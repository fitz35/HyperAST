use std::{
    collections::HashMap, fmt::Debug, marker::PhantomData, num::NonZeroU16, ops::Deref,
    ptr::NonNull,
};

use bitvec::order::Lsb0;
use num_traits::{cast, PrimInt};

use crate::{matchers::{decompressed_tree_store::{BreathFirst, BreathFirstContigousSiblings, BreathFirstIterable, DecompressedTreeStore, DecompressedWithParent, Initializable, PostOrder, PostOrderIterable, ShallowDecompressedTreeStore}, mapping_store::{DefaultMappingStore, MappingStore, MonoMappingStore}}, tree::{
        tree::{Labeled, NodeStore, Stored, WithChildren},
        tree_path::CompressedTreePath,
    }, utils::sequence_algorithms::longest_common_subsequence};

pub trait Actions {
    fn len(&self) -> usize;
}

pub struct ActionsVec<A>(Vec<A>);

#[derive(PartialEq, Eq)]
pub enum SimpleAction<Src, Dst, T: Stored + Labeled + WithChildren> {
    Delete {
        tree: Src,
    },
    Update {
        src: Src,
        dst: Dst,
        old: T::Label,
        new: T::Label,
    },
    Move {
        sub: Src,
        parent: Option<Dst>,
        idx: T::ChildIdx,
    },
    // Duplicate { sub: Src, parent: Dst, idx: T::ChildIdx },
    MoveUpdate {
        sub: Src,
        parent: Option<Dst>,
        idx: T::ChildIdx,
        old: T::Label,
        new: T::Label,
    },
    Insert {
        sub: T::TreeId,
        parent: Option<Dst>,
        idx: T::ChildIdx,
    },
}

impl<IdD> Actions for ActionsVec<IdD> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

pub trait TestActions<IdD, T: Stored + Labeled + WithChildren> {
    fn has_items(&self, items: &[SimpleAction<IdD, IdD, T>]) -> bool;
}

impl<T: Stored + Labeled + WithChildren + std::cmp::PartialEq, IdD: std::cmp::PartialEq>
    TestActions<IdD, T> for ActionsVec<SimpleAction<IdD, IdD, T>>
{
    fn has_items(&self, items: &[SimpleAction<IdD, IdD, T>]) -> bool {
        items.iter().all(|x| self.0.contains(x))
    }
}

/// try to use it to differentiate src and dst situations
trait Duet {
    type Src;
    type Dst;
}

struct Inserted<IdC, IdD> {
    original: IdC,
    parent: IdD,
}

struct InOrderNodes<IdD>(Option<Vec<IdD>>);

struct SemVer {
    major: u8,
    minor: u8,
    patch: u8,
}

struct CommmitId(String);

pub trait VecStore {}

pub struct DenseVecStore<Idx, V> {
    v: Vec<V>,
    phantom: PhantomData<*const Idx>,
}

impl<Idx: ReservableIndex, T> core::ops::Index<Idx::Reserved> for DenseVecStore<Idx, T> {
    type Output = T;

    fn index(&self, index: Idx::Reserved) -> &Self::Output {
        &self.v[index.into()]
    }
}

pub struct SparseVecStore<Idx, V> {
    v: Vec<(Idx, V)>,
    phantom: PhantomData<*const Idx>,
}

mod internal {
    use super::*;
    pub trait InternalReservable: From<Self::T> + Into<usize> {
        type T;
    }
    impl<T: PrimInt> InternalReservable for Restricted<T> {
        type T = T;
    }
}

pub trait Reservable: internal::InternalReservable + Copy {}

#[derive(Copy, Clone)]
struct Restricted<T>(T);

impl<T: PrimInt> From<T> for Restricted<T> {
    fn from(x: T) -> Self {
        Restricted(cast(x).unwrap())
    }
}

impl<T: PrimInt> Into<usize> for Restricted<T> {
    fn into(self) -> usize {
        cast(self.0).unwrap()
    }
}

impl<T: PrimInt> Reservable for Restricted<T> {}

pub trait ReservableIndex {
    type Reserved: Reservable;
    type Unpacked;
    fn value(&self) -> Self::Unpacked;
}

struct VersionIndex<Idx> {
    value: Idx,
    phantom: PhantomData<*const Idx>,
}

enum UnpackedVersionIndex<Idx> {
    FirstCommit,
    Default(Idx),
    LastCommit,
}

impl<Idx: PrimInt> ReservableIndex for VersionIndex<Idx> {
    type Reserved = Restricted<Idx>;
    type Unpacked = UnpackedVersionIndex<Self::Reserved>;

    fn value(&self) -> Self::Unpacked {
        if self.value == num_traits::Bounded::max_value() {
            Self::Unpacked::FirstCommit
        } else if self.value + num_traits::one() == num_traits::Bounded::max_value() {
            Self::Unpacked::LastCommit
        } else {
            Self::Unpacked::Default(Restricted(self.value))
        }
    }
}

struct Versions<IdV: ReservableIndex> {
    // names: DenseVecStore<IdV, SemVer>,
    commits: DenseVecStore<IdV, CommmitId>,
    first_parents: DenseVecStore<IdV, IdV>,
    second_parents: SparseVecStore<IdV, IdV>,
    other_parents: SparseVecStore<IdV, IdV>,
}

fn f(x: Versions<VersionIndex<u16>>) {
    let b = Restricted::from(0);
    let a = &x.commits[b];
    let c = &x.first_parents[b];
    let d = match c.value() {
        UnpackedVersionIndex::FirstCommit => UnpackedVersionIndex::FirstCommit,
        UnpackedVersionIndex::Default(i) => UnpackedVersionIndex::Default(&x.first_parents[i]),
        UnpackedVersionIndex::LastCommit => UnpackedVersionIndex::LastCommit,
    };
}

struct MegaTreeStore<IdC> {
    projects: Vec<SuperTreeStore<IdC>>,
}
struct Versioned<IdV, T> {
    insert: VersionIndex<IdV>,
    delete: VersionIndex<IdV>,
    content: T,
}
struct Descendant<T> {
    path: CompressedTreePath<u16>,
    tree: T,
}
// type IdC = u32;
type IdV = u16;

enum SuperTree<IdC> {
    InsertionsPhase {
        node: Box<SuperTree<IdC>>,
        insert: VersionIndex<IdV>,
        descendants: Vec<Descendant<IdC>>,
    },
    ManyVersion {
        node: IdC,
        children: Vec<Versioned<IdV, SuperTree<IdC>>>,
    },
    ManyFarVersion {
        node: IdC,
        descendants: Vec<Versioned<IdV, Descendant<SuperTree<IdC>>>>,
    },
    Far {
        node: IdC,
        descendants: Vec<Descendant<SuperTree<IdC>>>,
    },
    FixedChildren {
        node: IdC,
        children: Box<[SuperTree<IdC>]>,
    },
    CompressedFixedDiamond {
        node: IdC,
        children: Box<[Versioned<IdV, IdC>]>,
    },
    Basic {
        node: IdC,
    },
}
struct SuperTreeStore<IdC> {
    versions: Versions<VersionIndex<u16>>,
    root: SuperTree<IdC>,
    // can always be used as src ?
    // can "split" actions
    // should be easy to traverse in post order if used as src
    // should be easy to traverse in bfs is used as dst
    // should be able to insert new subtrees
    //                   delete old ones
    //                   materialize moves
    //                   duplicates
    // should allow easy reserialize at any version
    //        or combination of elements from different versions

    // a good middle ground would be to use Rc<> for higher nodes
    // also maybe nodes with a path, thus no need to dup nodes not changed
}

impl<IdC> SuperTreeStore<IdC> {
    fn from_version_and_path(
        &self,
        version: VersionIndex<u16>,
        path: CompressedTreePath<u32>,
    ) -> SuperTree<IdC> {
        // self.root.;

        todo!()
    }
    // post_order accessors
    // *****_in_post_order

    // bfs accessors
}

/// id for nodes in multi ast
// type IdM = u32;
type Label = u16;

/// FEATURE: share parents
static COMPRESSION: bool = false;

pub struct ScriptGenerator<
    'a,
    IdD: PrimInt + Debug,
    T: Stored + Labeled + WithChildren,
    SS, //:DecompressedTreeStore<T::TreeId, IdD> + DecompressedWithParent<IdD>,
    SD: BreathFirstIterable<'a,T::TreeId, IdD> + DecompressedWithParent<IdD>,
    S: NodeStore<T>,
> {
    store: &'a S,
    // origMappings: &'a DefaultMappingStore<IdD>,
    origDst: IdD,
    src_arena: &'a SS,
    mid_arena: (),//SuperTreeStore<T::TreeId>,
    dst_arena: &'a SD,
    // ori_to_copy: DefaultMappingStore<IdD>,
    cpyMappings: DefaultMappingStore<IdD>,
    inserted: Vec<Inserted<T::TreeId, IdD>>,
    actions: ActionsVec<SimpleAction<IdD, IdD, T>>,

    srcInOrder: InOrderNodes<IdD>,
    dstInOrder: InOrderNodes<IdD>,
}

impl<
        'a,
        IdD: PrimInt + Debug,
        T: Stored + Labeled + WithChildren,
        SS: DecompressedTreeStore<T::TreeId, IdD> + DecompressedWithParent<IdD> + PostOrder<T::TreeId,IdD>,
        SD: DecompressedTreeStore<T::TreeId, IdD>
            + DecompressedWithParent<IdD>
            + BreathFirstIterable<'a,T::TreeId, IdD>,
        S: NodeStore<T>,
    > ScriptGenerator<'a, IdD, T, SS, SD, S>
{
    pub fn compute_actions(
        store: &'a S,
        src_arena: &'a SS,
        dst_arena: &'a SD,
        ms: &'a DefaultMappingStore<IdD>,
    ) -> ActionsVec<SimpleAction<IdD, IdD, T>> {
        Self::new(store, src_arena, dst_arena)
            .init_cpy(ms)
            .generate()
            .actions
    }

    fn new(
        store: &'a S,
        src_arena: &'a SS,
        dst_arena: &'a SD,
        // ms: &'a DefaultMappingStore<IdD>,
    ) -> Self {
        Self {
            store,
            // origMappings: ms,
            origDst: src_arena.root(),
            src_arena,
            dst_arena,
            // ori_to_copy: DefaultMappingStore::new(),
            cpyMappings: DefaultMappingStore::new(),
            inserted: Default::default(),
            actions: ActionsVec::new(),
            srcInOrder: InOrderNodes(None),
            dstInOrder: InOrderNodes(None),
        }
    }

    fn init_cpy(mut self, ms: &'a DefaultMappingStore<IdD>) -> Self {
        // copy mapping
        self.cpyMappings = ms.clone();
        // copy src // no need here just use an insert list
        // relate src to copied src
        // for x in self.src_arena.iter() {
        //     self.ori_to_copy.link(x, x);
        // }
        self
    }

    fn generate(mut self) -> Self {
        // fake root ?
        // fake root link ?

        self.ins_mov_upd();

        self.del();
        self
    }

    fn ins_mov_upd(&mut self) {
        if COMPRESSION {
            todo!()
        }
        self.auxilary_ins_mov_upd();
    }

    fn auxilary_ins_mov_upd(&mut self) {
        for x in self.dst_arena.iter_bf() {
            let w;
            let y = self.dst_arena.parent(&x);
            let z = y.and_then(|y| Some(self.cpyMappings.get_src(&y)));

            if !self.cpyMappings.is_dst(&x) {
                // insertion
                let k = if let Some(y) = y  {
                    self.findPos(&x, &y)
                } else {
                    num_traits::zero()
                };
                w = self.make_inserted_node(&x, &z, &k);
                // self.apply_insert(&w, &z, &k);
                self.cpyMappings.link(w, x);
                let action = SimpleAction::Insert {
                    sub: self.dst_arena.original(&x),
                    parent: z,
                    idx: k,
                };
                self.apply_insert(&action);
                self.actions.push(action);
            } else {
                w = self.cpyMappings.get_src(&x);
                if x != self.origDst {
                    let v = self.src_arena.parent(&w);
                    let w_l = self
                        .store
                        .get_node_at_id(&self.src_arena.original(&w))
                        .get_label();
                    let x_l = self
                        .store
                        .get_node_at_id(&self.dst_arena.original(&x))
                        .get_label();

                    if w_l != x_l && z != v {
                        // rename + move
                        let k = if let Some(y) = y  {
                            self.findPos(&x, &y)
                        } else {
                            num_traits::zero()
                        };
                        // self.apply_insert(&w, &z, &k);
                        self.cpyMappings.link(w, x);
                        let action = SimpleAction::MoveUpdate {
                            sub: x,
                            parent: z,
                            idx: k,
                            old: w_l,
                            new: x_l,
                        };
                        self.apply_insert(&action);
                        self.actions.push(action);
                    } else if w_l != x_l {
                        // rename
                        self.cpyMappings.link(w, x);
                        // self.apply_update(&w, &z, &x_l);
                        let action = SimpleAction::Update {
                            src: w,
                            dst: x,
                            old: todo!(),
                            new: x_l,
                        };
                        self.apply_update(&action);
                        self.actions.push(action);
                    } else if z != v {
                        // move
                        let k = if let Some(y) = y  {
                            self.findPos(&x, &y)
                        } else {
                            num_traits::zero()
                        };
                        // self.apply_insert(&w, &z, &k);
                        self.cpyMappings.link(w, x);
                        let action = SimpleAction::Move {
                            sub: x,
                            parent: z,
                            idx: k,
                        };
                        self.apply_insert(&action);
                        self.actions.push(action);
                    } else {
                        // not changed
                        // and no changes to parents
                        // postentially try to share parent in super ast
                        if COMPRESSION {
                            todo!()
                        }
                    }
                    self.mdForMiddle(&x, &w);
                }
            }

            self.srcInOrder.push(w);
            self.dstInOrder.push(x);
            self.alignChildren(&w, &x);
        }
    }

    fn del(&mut self) {
        for w in self.iterCpySrcInPostOrder() {
            if self.cpyMappings.is_src(&w) {
                let action = SimpleAction::Delete { tree: w };
                self.apply_delete(&action);
                self.actions.push(action);
            } else {
                // not modified
                // all parents were not modified
                // maybe do the resources sharing now
                if COMPRESSION {
                    todo!()
                }
            }
        }
        if COMPRESSION {
            // postorder compression ?
            todo!()
        }
    }

    pub(crate) fn alignChildren(&mut self, w: &IdD, x: &IdD) {
        let w_c = self.src_arena.children(self.store, w);
        self.srcInOrder.removeAll(&w_c);
        let x_c = self.dst_arena.children(self.store, x);
        self.dstInOrder.removeAll(&x_c);

        let mut s1 = vec![];
        for c in &w_c {
            if self.cpyMappings.is_src(c) {
                if w_c.contains(&self.cpyMappings.get_src(c)) {
                    s1.push(*c);
                }
            }
        }
        let mut s2 = vec![];
        for c in &x_c {
            if self.cpyMappings.is_dst(c) {
                if x_c.contains(&self.cpyMappings.get_dst(c)) {
                    s2.push(*c);
                }
            }
        }

        let lcs = self.lcs(&s1, &s2);

        for m in &lcs {
            self.srcInOrder.push(m.0);
            self.dstInOrder.push(m.1);
        }
        for a in &s1 {
            for b in &s2 {
                if self.cpyMappings.has(&a, &b) && !lcs.contains(&(*a, *b)) {
                    let k = self.findPos(b, x);
                    let action = SimpleAction::Move {
                        sub: *a,
                        parent: Some(*w),
                        idx: k,
                    };
                    self.apply_move(&action);
                    self.actions.push(action);
                    self.srcInOrder.push(*a);
                    self.dstInOrder.push(*b);
                }
            }
        }
    }

    /// find position of x in parent on dst_arena
    pub(crate) fn findPos(&self, x: &IdD, parent: &IdD) -> T::ChildIdx {
        let y = parent;
        let siblings = self.dst_arena.children(self.store, y);

        for c in &siblings {
            if self.dstInOrder.contains(c) {
                if c == x {
                    return num_traits::zero();
                } else {
                    break;
                }
            }
        }
        let xpos = cast(self.src_arena.position_in_parent(self.store, x)).unwrap(); //child.positionInParent();
        let mut v: Option<IdD> = None;
        for i in 0..xpos {
            let c: &IdD = &siblings[i];
            if self.dstInOrder.contains(c) {
                v = Some(*c);
            };
        }

        if v.is_none() {
            return num_traits::zero();
        }

        let u = self.cpyMappings.get_src(&v.unwrap());
        let upos = self.src_arena.position_in_parent(self.store, &u);
        upos + num_traits::one()
    }

    pub(crate) fn lcs(&self, src_children: &[IdD], dst_children: &[IdD]) -> Vec<(IdD, IdD)> {
        longest_common_subsequence(src_children, dst_children, |src, dst| {
            self.cpyMappings.has(src, dst)
        })
    }

    pub(crate) fn mdForMiddle(&self, x: &IdD, w: &IdD) {
        // todo maybe later
    }

    pub(crate) fn make_inserted_node(&self, x: &IdD, z: &Option<IdD>, k: &T::ChildIdx) -> IdD {
        // self.inserted.push(value);
        todo!();
        cast(self.src_arena.len() + self.inserted.len()-1).unwrap()
    }

    pub(crate) fn apply_insert(&self, a: &SimpleAction<IdD, IdD, T>) {
        todo!()
    }

    pub(crate) fn apply_update(&self, a: &SimpleAction<IdD, IdD, T>) {
        todo!()
    }

    pub(crate) fn apply_delete(&self, a: &SimpleAction<IdD, IdD, T>) {
        todo!()
    }

    pub(crate) fn apply_move(&self, action: &SimpleAction<IdD, IdD, T>) {
        // let oldk = self.src_arena.child_postion(&a);
        todo!()
    }

    pub(crate) fn iterCpySrcInPostOrder(&self) -> Vec<IdD> {
        todo!()
    }
}

// pub(crate) struct SS<IdC, IdD> {
//     a: IdD,
//     back: IdC,
// }

// impl<IdC, IdD> SS<IdC, IdD> {
//     pub(crate) fn parent(&self, w: &IdD) -> Option<IdD> {
//         todo!()
//     }

//     // pub(crate) fn label(&self, w: &IdD) -> Label {
//     //     todo!()
//     // }

//     fn children(&self, w: &IdD) -> Vec<IdD> {
//         todo!()
//     }

//     pub(crate) fn child_postion(&self, a: &IdD) -> usize {
//         todo!()
//     }

//     pub(crate) fn original(&self, x: &IdD) -> IdC {
//         todo!()
//     }
// }

impl<T: Stored + Labeled + WithChildren, IdD> ActionsVec<SimpleAction<IdD, IdD, T>> {
    pub(crate) fn push(&mut self, action: SimpleAction<IdD, IdD, T>) {
        self.0.push(action)
    }

    pub(crate) fn new() -> Self {
        Self(Default::default())
    }
}

impl<IdD: Eq> InOrderNodes<IdD> {
    /// TODO add precondition to try to linerarly remove element (if both ordered the same way it's easy to remove without looking multiple times in both lists)
    fn removeAll(&mut self, w: &[IdD]) {
        if let Some(a) = self.0.take() {
            self.0 = Some(a.into_iter().filter(|x| w.contains(x)).collect());
        }
    }

    pub(crate) fn push(&mut self, x: IdD) {
        if let Some(l) = self.0.as_mut() {
            l.push(x)
        } else {
            self.0 = Some(vec![x])
        }
    }

    fn contains(&self, x: &IdD) -> bool {
        if let Some(l) = &self.0 {
            l.contains(x)
        } else {
            false
        }
    }
}
