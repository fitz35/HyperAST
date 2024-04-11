///! fully compress all subtrees from a tree-sitter query CST
use std::{collections::HashMap, fmt::Debug};

use crate::{types::TIdN, TNode};
use legion::world::EntryRef;

use hyper_ast::{
    filter::BloomSize,
    full::FullNode,
    hashed::{self, IndexingHashBuilder, MetaDataHashsBuilder, SyntaxNodeHashs},
    nodes::Space,
    store::{
        nodes::{
            legion::{
                compo::{self, NoSpacesCS, CS},
                HashedNodeRef, NodeIdentifier,
            },
            DefaultNodeStore as NodeStore,
        },
        SimpleStores,
    },
    tree_gen::{
        compute_indentation, get_spacing, has_final_space, parser::Node as _, AccIndentation,
        Accumulator, BasicAccumulator, BasicGlobalData, GlobalData, Parents, SpacedGlobalData,
        Spaces, SubTreeMetrics, TextedGlobalData, TreeGen, ZippedTreeGen,
    },
    types::LabelStore as _,
};

use crate::types::{TsQueryEnabledTypeStore, Type};

pub type LabelIdentifier = hyper_ast::store::labels::DefaultLabelIdentifier;

pub struct TsQueryTreeGen<'store, 'cache, TS> {
    pub line_break: Vec<u8>,
    pub stores: &'store mut SimpleStores<TS>,
    pub md_cache: &'cache mut MDCache,
}

pub type MDCache = HashMap<NodeIdentifier, MD>;

// NOTE only keep compute intensive metadata (where space/time tradeoff is worth storing)
// eg. decls refs, maybe hashes but not size and height
// * metadata: computation results from concrete code of node and its children
// they can be qualitative metadata .eg a hash or they can be quantitative .eg lines of code
pub struct MD {
    metrics: SubTreeMetrics<SyntaxNodeHashs<u32>>,
}

impl From<Local> for MD {
    fn from(x: Local) -> Self {
        MD { metrics: x.metrics }
    }
}

pub type Global<'a> = SpacedGlobalData<'a>;

/// TODO temporary placeholder
#[derive(Debug, Clone, Default)]
pub struct PartialAnalysis {}

#[derive(Debug, Clone)]
pub struct Local {
    pub compressed_node: NodeIdentifier,
    pub metrics: SubTreeMetrics<SyntaxNodeHashs<u32>>,
}

impl Local {
    fn acc(self, acc: &mut Acc) {
        if self.metrics.size_no_spaces > 0 {
            acc.no_space.push(self.compressed_node)
        }
        acc.simple.push(self.compressed_node);
        acc.metrics.acc(self.metrics);

        // TODO things with this.ana
    }
}

pub struct Acc {
    simple: BasicAccumulator<Type, NodeIdentifier>,
    no_space: Vec<NodeIdentifier>,
    labeled: bool,
    start_byte: usize,
    end_byte: usize,
    metrics: SubTreeMetrics<SyntaxNodeHashs<u32>>,
    padding_start: usize,
    indentation: Spaces,
}

pub type FNode = FullNode<BasicGlobalData, Local>;
impl Accumulator for Acc {
    type Node = FNode;
    fn push(&mut self, full_node: Self::Node) {
        full_node.local.acc(self);
    }
}

impl AccIndentation for Acc {
    fn indentation<'a>(&'a self) -> &'a Spaces {
        &self.indentation
    }
}

#[repr(transparent)]
pub struct TTreeCursor<'a>(tree_sitter::TreeCursor<'a>);

impl<'a> Debug for TTreeCursor<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TTreeCursor")
            .field(&self.0.node().kind())
            .finish()
    }
}
impl<'a> hyper_ast::tree_gen::parser::TreeCursor<'a, TNode<'a>> for TTreeCursor<'a> {
    fn node(&self) -> TNode<'a> {
        TNode(self.0.node())
    }

    fn goto_first_child(&mut self) -> bool {
        self.0.goto_first_child()
    }

    fn goto_parent(&mut self) -> bool {
        self.0.goto_parent()
    }

    fn goto_next_sibling(&mut self) -> bool {
        self.0.goto_next_sibling()
    }
}

pub fn tree_sitter_parse(text: &[u8]) -> Result<tree_sitter::Tree, tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_query::language();
    parser.set_language(language).unwrap();
    let tree = parser.parse(text, None).unwrap();
    if tree.root_node().has_error() {
        Err(tree)
    } else {
        Ok(tree)
    }
}

impl<'store, 'cache, TS: TsQueryEnabledTypeStore<HashedNodeRef<'store, TIdN<NodeIdentifier>>>>
    ZippedTreeGen for TsQueryTreeGen<'store, 'cache, TS>
{
    type Stores = SimpleStores<TS>;
    type Text = [u8];
    type Node<'b> = TNode<'b>;
    type TreeCursor<'b> = TTreeCursor<'b>;

    fn stores(&mut self) -> &mut Self::Stores {
        &mut self.stores
    }

    fn init_val(&mut self, text: &[u8], node: &Self::Node<'_>) -> Self::Acc {
        let type_store = &mut self.stores().type_store;
        let kind = node.obtain_type(type_store);
        let parent_indentation = Space::try_format_indentation(&self.line_break)
            .unwrap_or_else(|| vec![Space::Space; self.line_break.len()]);
        let indent = compute_indentation(
            &self.line_break,
            text,
            node.start_byte(),
            0,
            &parent_indentation,
        );
        let labeled = node.has_label();
        Acc {
            simple: BasicAccumulator {
                kind,
                children: vec![],
            },
            no_space: vec![],
            labeled,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            metrics: Default::default(),
            padding_start: 0,
            indentation: indent,
        }
    }
    fn pre_skippable(
        &mut self,
        text: &Self::Text,
        node: &Self::Node<'_>,
        stack: &Parents<Self::Acc>,
        global: &mut Self::Global,
        skip: &mut bool,
    ) -> Option<<Self as TreeGen>::Acc> {
        let type_store = &mut self.stores().type_store;
        // let kind = node.kind();
        // let Some(kind) = type_store.try_cpp(kind) else {
        //     return None
        // };
        let kind = node.obtain_type(type_store);
        // TODO remove this mitigations as it breaks captures on anonymous nodes
        if kind == Type::TS0 || kind == Type::AnonymousNode || kind == Type::String {
            *skip = true;
        }
        let mut acc = self.pre(text, node, stack, global);
        if kind == Type::TS0 || kind == Type::AnonymousNode || kind == Type::String {
            acc.labeled = true;
        }
        Some(acc)
    }
    fn pre(
        &mut self,
        text: &[u8],
        node: &Self::Node<'_>,
        stack: &Parents<Self::Acc>,
        global: &mut Self::Global,
    ) -> <Self as TreeGen>::Acc {
        let type_store = &mut self.stores().type_store;
        let parent_indentation = &stack.parent().unwrap().indentation();
        let kind = node.obtain_type(type_store);
        let indent = compute_indentation(
            &self.line_break,
            text,
            node.start_byte(),
            global.sum_byte_length(),
            &parent_indentation,
        );
        Acc {
            labeled: node.has_label(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            metrics: Default::default(),
            padding_start: global.sum_byte_length(),
            indentation: indent,
            simple: BasicAccumulator {
                kind,
                children: vec![],
            },
            no_space: vec![],
        }
    }

    fn post(
        &mut self,
        parent: &mut <Self as TreeGen>::Acc,
        global: &mut Self::Global,
        text: &[u8],
        acc: <Self as TreeGen>::Acc,
    ) -> <<Self as TreeGen>::Acc as Accumulator>::Node {
        let spacing = get_spacing(
            acc.padding_start,
            acc.start_byte,
            text,
            parent.indentation(),
        );
        if let Some(spacing) = spacing {
            parent.push(FullNode {
                global: global.into(),
                local: self.make_spacing(spacing),
            });
        }

        let label = if acc.labeled {
            std::str::from_utf8(&text[acc.start_byte..acc.end_byte])
                .ok()
                .map(|x| x.to_string())
        } else {
            None
        };
        self.make(global, acc, label)
    }
}

impl<'store, 'cache, TS: TsQueryEnabledTypeStore<HashedNodeRef<'store, TIdN<NodeIdentifier>>>>
    TsQueryTreeGen<'store, 'cache, TS>
{
    fn make_spacing(
        &mut self,
        spacing: Vec<u8>, //Space>,
    ) -> Local {
        let bytes_len = spacing.len();
        let spacing = std::str::from_utf8(&spacing).unwrap().to_string();
        let spacing_id = self.stores.label_store.get_or_insert(spacing.clone());
        let hbuilder: hashed::Builder<SyntaxNodeHashs<u32>> =
            hashed::Builder::new(Default::default(), &Type::Spaces, &spacing, 1);
        let hsyntax = hbuilder.most_discriminating();
        let hashable = &hsyntax;

        let eq = |x: EntryRef| {
            let t = x.get_component::<Type>();
            if t != Ok(&Type::Spaces) {
                return false;
            }
            let l = x.get_component::<LabelIdentifier>();
            if l != Ok(&spacing_id) {
                return false;
            }
            true
        };

        let insertion = self.stores.node_store.prepare_insertion(&hashable, eq);

        let mut hashs = hbuilder.build();
        hashs.structt = 0;
        hashs.label = 0;

        let compressed_node = if let Some(id) = insertion.occupied_id() {
            id
        } else {
            let vacant = insertion.vacant();
            let bytes_len = compo::BytesLen(bytes_len.try_into().unwrap());
            NodeStore::insert_after_prepare(
                vacant,
                (Type::Spaces, spacing_id, bytes_len, hashs, BloomSize::None),
            )
        };
        Local {
            compressed_node,
            metrics: SubTreeMetrics {
                size: 1,
                height: 1,
                hashs,
                size_no_spaces: 0,
            },
        }
    }

    pub fn new(
        stores: &'store mut <Self as ZippedTreeGen>::Stores,
        md_cache: &'cache mut MDCache,
    ) -> TsQueryTreeGen<'store, 'cache, TS> {
        TsQueryTreeGen::<'store, 'cache, TS> {
            line_break: "\n".as_bytes().to_vec(),
            stores,
            md_cache,
        }
    }

    pub fn generate_file(
        &mut self,
        name: &[u8],
        text: &'store [u8],
        cursor: tree_sitter::TreeCursor,
    ) -> FullNode<BasicGlobalData, Local> {
        let mut global = Global::from(TextedGlobalData::new(Default::default(), text));
        let mut init = self.init_val(text, &TNode(cursor.node()));
        let mut xx = TTreeCursor(cursor);

        let spacing = get_spacing(
            init.padding_start,
            init.start_byte,
            text,
            init.indentation(),
        );
        if let Some(spacing) = spacing {
            global.down();
            init.start_byte = 0;
            init.push(FullNode {
                global: global.into(),
                local: self.make_spacing(spacing),
            });
            global.right();
        }
        let mut stack = init.into();

        self.gen(text, &mut stack, &mut xx, &mut global);

        let mut acc = stack.finalize();

        if has_final_space(&0, global.sum_byte_length(), text) {
            let spacing = get_spacing(
                global.sum_byte_length(),
                text.len(),
                text,
                acc.indentation(),
            );
            if let Some(spacing) = spacing {
                global.right();
                acc.push(FullNode {
                    global: global.into(),
                    local: self.make_spacing(spacing),
                });
            }
        }
        let label = Some(std::str::from_utf8(name).unwrap().to_owned());
        let full_node = self.make(&mut global, acc, label);
        full_node
    }
}

pub fn eq_node<'a, K>(
    kind: &'a K,
    label_id: Option<&'a LabelIdentifier>,
    children: &'a [NodeIdentifier],
) -> impl Fn(EntryRef) -> bool + 'a
where
    K: 'static + Eq + std::hash::Hash + Copy + std::marker::Send + std::marker::Sync,
{
    move |x: EntryRef| {
        let t = x.get_component::<K>();
        if t != Ok(kind) {
            return false;
        }
        let l = x.get_component::<LabelIdentifier>().ok();
        if l != label_id {
            return false;
        } else {
            let cs = x.get_component::<CS<legion::Entity>>();
            let r = match cs {
                Ok(CS(cs)) => cs.as_ref() == children,
                Err(_) => children.is_empty(),
            };
            if !r {
                return false;
            }
        }
        true
    }
}

impl<
        'stores,
        'cache,
        TS: TsQueryEnabledTypeStore<HashedNodeRef<'stores, TIdN<NodeIdentifier>>>,
    > TreeGen for TsQueryTreeGen<'stores, 'cache, TS>
{
    type Acc = Acc;
    type Global = SpacedGlobalData<'stores>;
    fn make(
        &mut self,
        global: &mut <Self as TreeGen>::Global,
        acc: <Self as TreeGen>::Acc,
        label: Option<String>,
    ) -> <<Self as TreeGen>::Acc as Accumulator>::Node {
        let node_store = &mut self.stores.node_store;
        let label_store = &mut self.stores.label_store;
        let interned_kind =
            TsQueryEnabledTypeStore::intern(&self.stores.type_store, acc.simple.kind);
        let hashs = acc.metrics.hashs;
        let size = acc.metrics.size + 1;
        let height = acc.metrics.height + 1;
        let size_no_spaces = acc.metrics.size_no_spaces + 1;
        let hbuilder = hashed::Builder::new(hashs, &interned_kind, &label, size_no_spaces);
        let hsyntax = hbuilder.most_discriminating();
        let hashable = &hsyntax;

        let label_id = label
            .as_ref()
            .map(|label| label_store.get_or_insert(label.as_str()));
        let eq = eq_node(&interned_kind, label_id.as_ref(), &acc.simple.children);

        let insertion = node_store.prepare_insertion(&hashable, eq);

        let local = if let Some(compressed_node) = insertion.occupied_id() {
            let hashs = hbuilder.build();
            let metrics = SubTreeMetrics {
                size,
                height,
                hashs,
                size_no_spaces,
            };
            Local {
                compressed_node,
                metrics,
            }
        } else {
            let hashs = hbuilder.build();

            let mut dyn_builder =
                hyper_ast::store::nodes::legion::dyn_builder::EntityBuilder::new();
            dyn_builder.add(interned_kind);
            dyn_builder.add(hashs.clone());
            dyn_builder.add(compo::BytesLen(
                (acc.end_byte - acc.start_byte).try_into().unwrap(),
            ));
            if let Some(label_id) = label_id {
                dyn_builder.add(label_id);
            }
            match acc.simple.children.len() {
                0 => {}
                x => {
                    let a = acc.simple.children.into_boxed_slice();
                    dyn_builder.add(compo::Size(size));
                    dyn_builder.add(compo::SizeNoSpaces(size_no_spaces));
                    dyn_builder.add(compo::Height(height));
                    dyn_builder.add(CS(a));
                    if x != acc.no_space.len() {
                        dyn_builder.add(NoSpacesCS(acc.no_space.into_boxed_slice()));
                    }
                }
            }
            let compressed_node =
                NodeStore::insert_built_after_prepare(insertion.vacant(), dyn_builder.build());

            let metrics = SubTreeMetrics {
                size,
                height,
                hashs,
                size_no_spaces,
            };
            Local {
                compressed_node,
                metrics,
            }
        };

        let full_node = FullNode {
            global: global.into(),
            local,
        };
        full_node
    }
}

impl<'stores, 'cache> TsQueryTreeGen<'stores, 'cache, crate::types::TStore> {
    pub fn build_then_insert(
        &mut self,
        i: <hashed::HashedNode as hyper_ast::types::Stored>::TreeId,
        t: Type,
        l: Option<LabelIdentifier>,
        cs: Vec<NodeIdentifier>,
    ) -> NodeIdentifier {
        let mut acc: Acc = {
            let kind = t;
            Acc {
                labeled: l.is_some(),
                start_byte: 0,
                end_byte: 0,
                metrics: Default::default(),
                indentation: vec![],
                simple: BasicAccumulator {
                    kind,
                    children: vec![],
                },
                no_space: vec![],
                padding_start: 0,
            }
        };
        for c in cs {
            let local = {
                // print_tree_syntax(&self.stores.node_store, &self.stores.label_store, &c);
                // println!();
                let md = self.md_cache.get(&c);
                let metrics = if let Some(md) = md {
                    let metrics = md.metrics;
                    metrics
                } else {
                    use hyper_ast::hashed::SyntaxNodeHashsKinds;
                    use hyper_ast::types::WithHashs;
                    let (kind, node) = self.stores.node_store.resolve_with_type::<TIdN<NodeIdentifier>>(&c);
                    let hashs = SyntaxNodeHashs {
                        structt: WithHashs::hash(&node, &SyntaxNodeHashsKinds::Struct),
                        label: WithHashs::hash(&node, &SyntaxNodeHashsKinds::Label),
                        syntax: WithHashs::hash(&node, &SyntaxNodeHashsKinds::Syntax),
                    };
                    use hyper_ast::types::WithStats;
                    use num::ToPrimitive;
                    let metrics = SubTreeMetrics {
                        size: node.size().to_u32().unwrap(),
                        height: node.height().to_u32().unwrap(),
                        size_no_spaces: node.size_no_spaces().to_u32().unwrap(),
                        hashs,
                    };
                    metrics
                };
                Local {
                    compressed_node: c,
                    metrics,
                }
            };
            let global = BasicGlobalData::default();
            let full_node = FullNode { global, local };
            acc.push(full_node);
        }
        let node_store = &mut self.stores.node_store;
        let label_store = &mut self.stores.label_store;
        let interned_kind =
            TsQueryEnabledTypeStore::intern(&self.stores.type_store, acc.simple.kind);
        let hashs = acc.metrics.hashs;
        let size = acc.metrics.size + 1;
        let height = acc.metrics.height + 1;
        let size_no_spaces = acc.metrics.size_no_spaces + 1;

        let label = l.map(|l|label_store.resolve(&l));
        let hbuilder = hashed::Builder::new(hashs, &interned_kind, &label, size_no_spaces);
        let hsyntax = hbuilder.most_discriminating();
        let hashable = &hsyntax;

        let label_id = l;
        let eq = eq_node(&interned_kind, label_id.as_ref(), &acc.simple.children);

        let insertion = node_store.prepare_insertion(&hashable, eq);

        let local = if let Some(compressed_node) = insertion.occupied_id() {
            let hashs = hbuilder.build();
            let metrics = SubTreeMetrics {
                size,
                height,
                hashs,
                size_no_spaces,
            };
            Local {
                compressed_node,
                metrics,
            }
        } else {
            let hashs = hbuilder.build();

            let mut dyn_builder =
                hyper_ast::store::nodes::legion::dyn_builder::EntityBuilder::new();
            dyn_builder.add(interned_kind);
            dyn_builder.add(hashs.clone());
            dyn_builder.add(compo::BytesLen(
                (acc.end_byte - acc.start_byte).try_into().unwrap(),
            ));
            if let Some(label_id) = label_id {
                dyn_builder.add(label_id);
            }
            match acc.simple.children.len() {
                0 => {}
                x => {
                    let a = acc.simple.children.into_boxed_slice();
                    dyn_builder.add(compo::Size(size));
                    dyn_builder.add(compo::SizeNoSpaces(size_no_spaces));
                    dyn_builder.add(compo::Height(height));
                    dyn_builder.add(CS(a));
                    if x != acc.no_space.len() {
                        dyn_builder.add(NoSpacesCS(acc.no_space.into_boxed_slice()));
                    }
                }
            }
            let compressed_node =
                NodeStore::insert_built_after_prepare(insertion.vacant(), dyn_builder.build());

            let metrics = SubTreeMetrics {
                size,
                height,
                hashs,
                size_no_spaces,
            };
            Local {
                compressed_node,
                metrics,
            }
        };
        local.compressed_node
    }
}
