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
pub fn find_pairs(functions: &mut [FunctionUnit], threshold: f64, min_shared: u32) -> Vec<Pair> {
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
                    found.push(Pair { a: id, b: other_id });
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

/// A small function whose fingerprints are almost entirely contained in a
/// larger one — the "copied into a bigger function" case Jaccard misses,
/// because the larger body inflates the union while the intersection stays
/// bounded by the small one.
pub struct ContainmentPair {
    pub small: u32,
    pub large: u32,
    /// shared / |small fingerprints|: how much of the small function the
    /// larger one covers (0.0-1.0).
    pub containment: f64,
}

/// Find containment pairs the normal (Jaccard) scan misses: the smaller
/// function is at least `containment_threshold` covered by the larger one, yet
/// their Jaccard stays below `jaccard_threshold`. Runs on the post-cull
/// fingerprints, so it must be called *after* `find_pairs`. Pairs where one
/// function is lexically nested inside the other (a closure inside its parent)
/// are dropped: that overlap is structural, not a copy.
pub fn find_containment(
    functions: &[FunctionUnit],
    jaccard_threshold: f64,
    containment_threshold: f64,
    min_shared: u32,
) -> Vec<ContainmentPair> {
    let mut by_lang: FxHashMap<crate::lang::Lang, Vec<u32>> = FxHashMap::default();
    for (i, f) in functions.iter().enumerate() {
        by_lang.entry(f.lang).or_default().push(i as u32);
    }
    let mut out = Vec::new();
    for ids in by_lang.values() {
        out.extend(containment_within(
            functions,
            ids,
            jaccard_threshold,
            containment_threshold,
            min_shared,
        ));
    }
    out
}

fn containment_within(
    functions: &[FunctionUnit],
    ids: &[u32],
    jaccard_threshold: f64,
    containment_threshold: f64,
    min_shared: u32,
) -> Vec<ContainmentPair> {
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
                let (la, lb) = (me.fingerprints.len(), other.fingerprints.len());
                let union = la + lb - count as usize;
                let jaccard = if union == 0 {
                    0.0
                } else {
                    count as f64 / union as f64
                };
                // Above the Jaccard bar it is already a normal-scan cluster.
                if jaccard >= jaccard_threshold {
                    continue;
                }
                let min_len = la.min(lb);
                if min_len == 0 {
                    continue;
                }
                let containment = count as f64 / min_len as f64;
                if containment < containment_threshold {
                    continue;
                }
                // The smaller fingerprint set is the one being "contained".
                let (small, large) = if la <= lb {
                    (id, other_id)
                } else {
                    (other_id, id)
                };
                if lexically_nested(&functions[small as usize], &functions[large as usize]) {
                    continue;
                }
                found.push(ContainmentPair {
                    small,
                    large,
                    containment,
                });
            }
            found
        })
        .collect()
}

/// True when one function's byte range sits inside the other's within the same
/// file — a nested fn or closure that matches only because its parent encloses
/// it, not because it was copied.
fn lexically_nested(a: &FunctionUnit, b: &FunctionUnit) -> bool {
    let inside = |inner: &FunctionUnit, outer: &FunctionUnit| {
        inner.file == outer.file
            && outer.start_byte <= inner.start_byte
            && inner.end_byte <= outer.end_byte
    };
    inside(a, b) || inside(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn unit(file: u32, bytes: (u32, u32), fps: Vec<u64>) -> FunctionUnit {
        FunctionUnit {
            file,
            lang: Lang::Typescript,
            name: "f".into(),
            start_line: 1,
            end_line: 10,
            start_byte: bytes.0,
            end_byte: bytes.1,
            sig_lines: 10,
            fingerprints: fps,
            is_test: false,
            is_trait_impl_method: false,
        }
    }

    #[test]
    fn small_copied_into_large_is_reported() {
        // 10-fp function fully contained in a 30-fp one, in different files.
        // Jaccard = 10/30 = 0.33 (below 0.80), containment = 10/10 = 1.0.
        let small = (1..=10).collect::<Vec<u64>>();
        let large = (1..=30).collect::<Vec<u64>>();
        let functions = vec![unit(0, (0, 100), small), unit(1, (0, 900), large)];
        let pairs = find_containment(&functions, 0.80, 0.90, 3);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].small, 0);
        assert_eq!(pairs[0].large, 1);
        assert!((pairs[0].containment - 1.0).abs() < 1e-9);
    }

    #[test]
    fn lexically_nested_pair_is_skipped() {
        // Same file, small's byte range inside large's: a nested closure, not
        // a copy. Must not be reported even though containment is 1.0.
        let small = (1..=10).collect::<Vec<u64>>();
        let large = (1..=30).collect::<Vec<u64>>();
        let functions = vec![unit(0, (100, 400), small), unit(0, (0, 900), large)];
        let pairs = find_containment(&functions, 0.80, 0.90, 3);
        assert!(pairs.is_empty());
    }

    #[test]
    fn near_identical_pair_stays_with_jaccard_scan() {
        // Two same-size functions the normal scan already clusters (Jaccard
        // 1.0) are not re-reported as containment findings.
        let fps = (1..=10).collect::<Vec<u64>>();
        let functions = vec![unit(0, (0, 100), fps.clone()), unit(1, (0, 100), fps)];
        let pairs = find_containment(&functions, 0.80, 0.90, 3);
        assert!(pairs.is_empty());
    }
}
