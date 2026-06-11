//! Inverted fingerprint index and candidate pair generation. This is the
//! O(n²) firewall: functions only ever get compared when they already share
//! fingerprints, and ultra-common fingerprints (language boilerplate) are
//! culled before they can pair everything with everything.

use crate::extract::FunctionUnit;
use crate::fingerprint::jaccard;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

pub struct Pair {
    pub a: u32,
    pub b: u32,
    pub similarity: f64,
}

/// Cull fingerprints shared by more than this many functions: they are
/// idiom, not duplication (think `if err != nil` ladders). The cap scales
/// with corpus size but never drops below 100.
fn cull_cap(function_count: usize) -> usize {
    (function_count / 200).max(100)
}

/// Find all pairs of functions with Jaccard similarity >= `threshold`.
/// Functions are only compared within the same language. As a side effect,
/// culled fingerprints are removed from every function's fingerprint set so
/// downstream similarity math stays consistent.
pub fn find_pairs(
    functions: &mut [FunctionUnit],
    threshold: f64,
    min_shared: u32,
) -> Vec<Pair> {
    let mut by_lang: FxHashMap<crate::lang::Lang, Vec<u32>> = FxHashMap::default();
    for (i, f) in functions.iter().enumerate() {
        by_lang.entry(f.lang).or_default().push(i as u32);
    }

    let mut pairs = Vec::new();
    for ids in by_lang.values() {
        cull(functions, ids);
        pairs.extend(pairs_within(functions, ids, threshold, min_shared));
    }
    pairs
}

fn cull(functions: &mut [FunctionUnit], ids: &[u32]) {
    let cap = cull_cap(ids.len());
    let mut counts: FxHashMap<u64, u32> = FxHashMap::default();
    for &id in ids {
        for &fp in &functions[id as usize].fingerprints {
            *counts.entry(fp).or_insert(0) += 1;
        }
    }
    let culled: FxHashSet<u64> = counts
        .into_iter()
        .filter(|&(_, n)| n as usize > cap)
        .map(|(fp, _)| fp)
        .collect();
    if culled.is_empty() {
        return;
    }
    for &id in ids {
        functions[id as usize]
            .fingerprints
            .retain(|fp| !culled.contains(fp));
    }
}

fn pairs_within(
    functions: &[FunctionUnit],
    ids: &[u32],
    threshold: f64,
    min_shared: u32,
) -> Vec<Pair> {
    // Posting lists hold positions within `ids` (ascending), so candidate
    // generation can restrict to strictly-later functions and count each
    // pair exactly once.
    let mut postings: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
    for (pos, &id) in ids.iter().enumerate() {
        for &fp in &functions[id as usize].fingerprints {
            postings.entry(fp).or_default().push(pos as u32);
        }
    }

    ids.par_iter()
        .enumerate()
        .flat_map_iter(|(pos, &id)| {
            let me = &functions[id as usize];
            let mut shared: FxHashMap<u32, u32> = FxHashMap::default();
            for fp in &me.fingerprints {
                if let Some(list) = postings.get(fp) {
                    // Lists are sorted; only look at functions after `pos`.
                    let start = list.partition_point(|&p| p <= pos as u32);
                    for &other in &list[start..] {
                        *shared.entry(other).or_insert(0) += 1;
                    }
                }
            }
            let mut found = Vec::new();
            for (other_pos, count) in shared {
                if count < min_shared {
                    continue;
                }
                let other_id = ids[other_pos as usize];
                let other = &functions[other_id as usize];
                // Fingerprint vecs are distinct sets, so the shared count
                // *is* the exact intersection size.
                let union = me.fingerprints.len() + other.fingerprints.len() - count as usize;
                let similarity = if union == 0 {
                    0.0
                } else {
                    count as f64 / union as f64
                };
                if similarity >= threshold {
                    found.push(Pair {
                        a: id,
                        b: other_id,
                        similarity,
                    });
                }
            }
            found
        })
        .collect()
}

/// Similarity of two specific functions over their (post-cull) fingerprints.
pub fn similarity(a: &FunctionUnit, b: &FunctionUnit) -> f64 {
    jaccard(&a.fingerprints, &b.fingerprints)
}
