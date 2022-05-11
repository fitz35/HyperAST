use crate::{
    matchers::{
        decompressed_tree_store::{
            CompletePostOrder, Initializable as _, ShallowDecompressedTreeStore,
        },
        heuristic::gt::{
            bottom_up_matcher::BottomUpMatcher,
            greedy_bottom_up_matcher::GreedyBottomUpMatcher,
            greedy_subtree_matcher::{GreedySubtreeMatcher, SubtreeMatcher},
        },
        mapping_store::{DefaultMappingStore, MappingStore},
    },
    tests::{
        examples::{example_bottom_up, example_gumtree},
    },
    tree::{simple_tree::{vpair_to_stores, Tree, NS}},
};

#[test]
fn test_min_height_threshold() {
    let (_label_store, node_store, src, dst) = vpair_to_stores(example_gumtree());
    let mappings = DefaultMappingStore::new();
    // GreedySubtreeMatcher.MIN_HEIGHT = 0;
    let mapper = GreedySubtreeMatcher::<CompletePostOrder<_, u16>, _, _, _, 0>::matchh(
        &node_store,
        &src,
        &dst,
        mappings,
    );
    let SubtreeMatcher {
        src_arena,
        dst_arena,
        mappings: ms1,
        ..
    } = mapper.into();

    {
        let src = &src_arena.root();
        let dst = &dst_arena.root();
        let from_src = |path: &[u8]| src_arena.child(&node_store, src, path);
        let from_dst = |path: &[u8]| dst_arena.child(&node_store, dst, path);

        assert!(ms1.has(&from_src(&[1]), &from_dst(&[0])));
        assert!(ms1.has(&from_src(&[1, 0]), &from_dst(&[0, 0])));
        assert!(ms1.has(&from_src(&[1, 1]), &from_dst(&[0, 1])));
        assert!(ms1.has(&from_src(&[2]), &from_dst(&[2])));
        assert_eq!(4, ms1.len());
    }
    let mappings = DefaultMappingStore::new();
    // GreedySubtreeMatcher.MIN_HEIGHT = 1;
    let mapper = GreedySubtreeMatcher::<CompletePostOrder<_, u16>, _, _, _, 1>::matchh(
        &node_store,
        &src,
        &dst,
        mappings,
    );
    let SubtreeMatcher {
        src_arena,
        dst_arena,
        mappings: ms2,
        ..
    } = mapper.into();
    let src = &src_arena.root();
    let dst = &dst_arena.root();

    let from_src = |path: &[u8]| src_arena.child(&node_store, src, path);
    let from_dst = |path: &[u8]| dst_arena.child(&node_store, dst, path);

    assert!(ms2.has(&from_src(&[1]), &from_dst(&[0])));
    assert!(ms2.has(&from_src(&[1, 0]), &from_dst(&[0, 0])));
    assert!(ms2.has(&from_src(&[1, 1]), &from_dst(&[0, 1])));
    assert_eq!(3, ms2.len());
}

#[test]
fn test_sim_and_size_threshold() {
    let (label_store, node_store, src, dst) = vpair_to_stores(example_bottom_up());
    let mut ms: DefaultMappingStore<u16> = DefaultMappingStore::new();
    let src = &src;
    let dst = &dst;

    let src_arena = CompletePostOrder::<_, u16>::new(&node_store, src);
    let dst_arena = CompletePostOrder::<_, u16>::new(&node_store, dst);
    let src = &(src_arena.root());
    let dst = &(dst_arena.root());
    let from_src = |path: &[u8]| src_arena.child(&node_store, src, path);
    let from_dst = |path: &[u8]| dst_arena.child(&node_store, dst, path);
    println!("rootsrc: {:?}", src);
    println!("rootdst: {:?}", dst);

    ms.topit(src_arena.len() + 1, dst_arena.len() + 1);
    ms.link(from_src(&[0, 2, 0]), from_dst(&[0, 2, 0]));
    ms.link(from_src(&[0, 2, 1]), from_dst(&[0, 2, 1]));
    ms.link(from_src(&[0, 2, 2]), from_dst(&[0, 2, 2]));
    ms.link(from_src(&[0, 2, 3]), from_dst(&[0, 2, 3]));
    for (f, s) in ms.iter() {
        assert!(ms.has(&f, &s), "{} -x-> {}", f, s);
    }
    let ms1 = ms.clone();
    for (f, s) in ms.iter() {
        assert!(ms1.has(&f, &s), "{} -x-> {}", f, s);
    }

    let mut mapper =
        GreedyBottomUpMatcher::<CompletePostOrder<u16, u16>, _, _, NS<Tree>, _, 0, 1, 1>::new(
            &node_store,
            &label_store,
            src_arena,
            dst_arena,
            ms1,
        );
    GreedyBottomUpMatcher::execute(&mut mapper);

    let BottomUpMatcher {
        src_arena,
        dst_arena,
        mappings: ms1,
        ..
    } = mapper.into();
    let src = src_arena.root();
    let dst = dst_arena.root();

    // // assertEquals(5, ms1.size());
    assert_eq!(5, ms1.src_to_dst.iter().filter(|x| **x != 0).count());
    assert_eq!(5, ms1.len());
    for (f, s) in ms.iter() {
        assert!(ms1.has(&f, &s), "{} -x-> {}", f, s);
    }
    assert!(ms1.has(&src, &dst));

    let ms2 = ms.clone();
    let mut mapper = GreedyBottomUpMatcher::<_, _, _, NS<Tree>, _, 0, 1, 2>::new(
        &node_store,
        &label_store,
        src_arena,
        dst_arena,
        ms2,
    );
    GreedyBottomUpMatcher::execute(&mut mapper);
    let BottomUpMatcher {
        src_arena,
        dst_arena,
        mappings: ms2,
        ..
    } = mapper.into();
    let src = &src_arena.root();
    let dst = &dst_arena.root();
    let from_src = |path: &[u8]| src_arena.child(&node_store, src, path);
    let from_dst = |path: &[u8]| dst_arena.child(&node_store, dst, path);
    assert!(ms2.has(src, dst));
    for (f, s) in ms.iter() {
        assert!(ms2.has(&f, &s));
    }
    assert!(ms2.has(&from_src(&[0]), &from_dst(&[0])));
    assert!(ms2.has(&from_src(&[0, 2]), &from_dst(&[0, 2])));
    assert_eq!(7, ms2.len());

    let ms3 = ms.clone();
    let mut mapper = GreedyBottomUpMatcher::<_, _, _, NS<Tree>, _, 10, 1, 2>::new(
        &node_store,
        &label_store,
        src_arena,
        dst_arena,
        ms3,
    );
    GreedyBottomUpMatcher::execute(&mut mapper);
    let BottomUpMatcher {
        src_arena,
        dst_arena,
        mappings: ms3,
        ..
    } = mapper.into();
    let src = &src_arena.root();
    let dst = &dst_arena.root();
    let from_src = |path: &[u8]| src_arena.child(&node_store, src, path);
    let from_dst = |path: &[u8]| dst_arena.child(&node_store, dst, path);
    assert_eq!(9, ms3.len());
    for (f, s) in ms.iter() {
        assert!(ms3.has(&f, &s));
    }
    assert!(ms3.has(src, dst));
    assert!(ms3.has(&from_src(&[0]), &from_dst(&[0])));
    assert!(ms3.has(&from_src(&[0, 0]), &from_dst(&[0, 0])));
    assert!(ms3.has(&from_src(&[0, 1]), &from_dst(&[0, 1])));
    assert!(ms3.has(&from_src(&[0, 2]), &from_dst(&[0, 2])));
}

// test mapping stores

// @Test
// public void testMappingStore() {
//     ITree t1 = new Tree(TypeSet.type("foo"));
//     ITree t2 = new Tree(TypeSet.type("foo"));
//     MappingStore ms = new MappingStore(t1, t2);
//     assertEquals(0, ms.size());
//     assertFalse(ms.isSrcMapped(t1));
//     assertFalse(ms.isDstMapped(t2));
//     ms.addMapping(t1, t2);
//     assertEquals(1, ms.size());
//     assertTrue(ms.isSrcMapped(t1));
//     assertTrue(ms.isDstMapped(t2));
//     assertFalse(ms.areBothUnmapped(t1, t2));
//     ITree t3 = new Tree(TypeSet.type("foo"));
//     ITree t4 = new Tree(TypeSet.type("foo"));
//     assertFalse(ms.areSrcsUnmapped(Arrays.asList(new ITree[] {t1, t3})));
//     assertFalse(ms.areDstsUnmapped(Arrays.asList(new ITree[] {t2, t4})));
//     assertFalse(ms.areBothUnmapped(t1, t3));
//     assertFalse(ms.areBothUnmapped(t3, t2));
//     assertTrue(ms.areBothUnmapped(t3, t4));
//     Mapping m = ms.asSet().iterator().next();
//     assertEquals(t1, m.first);
//     assertEquals(t2, m.second);
//     ms.removeMapping(t1, t2);
//     assertEquals(0, ms.size());
//     assertTrue(ms.areSrcsUnmapped(Arrays.asList(new ITree[] {t1, t3})));
//     assertTrue(ms.areDstsUnmapped(Arrays.asList(new ITree[] {t2, t4})));
//     t3.setParentAndUpdateChildren(t1);
//     t4.setParentAndUpdateChildren(t2);
//     ms.addMappingRecursively(t1, t2);
//     assertEquals(2, ms.size());
//     assertTrue(ms.has(t1, t2));
//     assertTrue(ms.has(t3, t4));
// }

// @Test
// public void testMultiMappingStore() {
//     MultiMappingStore ms = new MultiMappingStore();
//     ITree t1 = new Tree(TypeSet.type("foo"));
//     ITree t2 = new Tree(TypeSet.type("foo"));
//     ms.addMapping(t1, t2);
//     assertEquals(1, ms.size());
//     assertTrue(ms.has(t1, t2));
//     assertTrue(ms.isSrcUnique(t1));
//     assertTrue(ms.isDstUnique(t2));
//     ITree t3 = new Tree(TypeSet.type("foo"));
//     ITree t4 = new Tree(TypeSet.type("foo"));
//     ms.addMapping(t3, t4);
//     assertEquals(2, ms.size());
//     assertTrue(ms.has(t3, t4));
//     assertTrue(ms.isSrcUnique(t3));
//     assertTrue(ms.isDstUnique(t4));
//     ms.addMapping(t1, t4);
//     System.out.println(ms);
//     assertEquals(3, ms.size());
//     assertTrue(ms.has(t1, t4));
//     assertFalse(ms.isSrcUnique(t1));
//     assertFalse(ms.isDstUnique(t4));
//     assertTrue(ms.isSrcUnique(t3));
//     assertTrue(ms.isDstUnique(t2));
//     ms.removeMapping(t1, t4);
//     assertEquals(2, ms.size());
//     assertTrue(ms.isSrcUnique(t1));
//     assertTrue(ms.isDstUnique(t2));
//     assertTrue(ms.isSrcUnique(t3));
//     assertTrue(ms.isDstUnique(t4));
// }
