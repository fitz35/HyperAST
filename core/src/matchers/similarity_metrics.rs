use std::{collections::HashSet, marker::PhantomData};

use bitvec::{order::Lsb0, store::BitStore};
use num_traits::{cast, PrimInt};

use super::mapping_store::MonoMappingStore;

pub fn chawathe_similarity<Id: PrimInt, Store: MonoMappingStore<Ele = Id>>(
    src: &[Id],
    dst: &[Id],
    mappings: &Store,
) -> f64 {
    let max = f64::max(src.len() as f64, dst.len() as f64);
    number_of_common_descendants(src, dst, mappings) as f64 / max
}

pub fn overlap_similarity<Id: PrimInt, Store: MonoMappingStore<Ele = Id>>(
    src: &[Id],
    dst: &[Id],
    mappings: &Store,
) -> f64 {
    let min = f64::min(src.len() as f64, dst.len() as f64);
    number_of_common_descendants(src, dst, mappings) as f64 / min
}

pub fn dice_similarity<Id: PrimInt, Store: MonoMappingStore<Ele = Id>>(
    src: &[Id],
    dst: &[Id],
    mappings: &Store,
) -> f64 {
    let common_descendants = number_of_common_descendants(src, dst, mappings) as f64;
    (2.0_f64 * common_descendants) / (src.len() as f64 + dst.len() as f64)
}

pub fn jaccard_similarity<Id: PrimInt, Store: MonoMappingStore<Ele = Id>>(
    src: &[Id],
    dst: &[Id],
    mappings: &Store,
) -> f64 {
    let num = number_of_common_descendants(src, dst, mappings) as f64;
    let den = src.len() as f64 + dst.len() as f64 - num;
    num / den
}

fn number_of_common_descendants<Id: PrimInt, Store: MonoMappingStore<Ele = Id>>(
    src: &[Id],
    dst: &[Id],
    mappings: &Store,
) -> u32 {
    let min: usize = cast(dst[0]).unwrap();
    let max: usize = cast::<_, usize>(dst[dst.len() - 1]).unwrap() + 1;
    let mut a = bitvec::bitvec![0;max-min];
    dst.iter()
        .for_each(|x| a.set(cast::<Id, usize>(*x).unwrap() - min, true));
    let dst_descendants: bitvec::boxed::BitBox = a.into_boxed_bitslice();
    let mut common = 0;

    for t in src {
        if mappings.is_src(t) {
            let m = mappings.get_dst(t);
            if dst_descendants[cast::<Id, usize>(m).unwrap() - min] {
                common += 1;
            }
        }
    }

    return common;
}
