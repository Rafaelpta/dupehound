//! Winnowing fingerprints over normalized token streams, after
//! Schleimer, Wilkerson & Aiken, "Winnowing: Local Algorithms for Document
//! Fingerprinting" (SIGMOD 2003) — the algorithm behind Stanford's MOSS.
//!
//! With k-gram size `k` and window size `w`, winnowing guarantees that any
//! run of at least t = w + k - 1 shared tokens produces at least one shared
//! fingerprint, while never fingerprinting a match shorter than k tokens.

/// Rabin-Karp base, a prime comfortably above the u16 token alphabet.
const BASE: u64 = 1_000_003;

/// splitmix64 finalizer: spreads the polynomial hashes uniformly so the
/// per-window minimum acts as a uniform random sample (what the winnowing
/// density bound 2/(w+1) assumes).
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Hash every k-gram of `codes` with a rolling polynomial hash, O(n).
fn kgram_hashes(codes: &[u16], k: usize) -> Vec<u64> {
    if codes.len() < k {
        return Vec::new();
    }
    // +1 so the token code 0 still contributes to the polynomial.
    let mut pow = 1u64;
    for _ in 0..k - 1 {
        pow = pow.wrapping_mul(BASE);
    }
    let mut hashes = Vec::with_capacity(codes.len() - k + 1);
    let mut h = 0u64;
    for &c in &codes[..k] {
        h = h.wrapping_mul(BASE).wrapping_add(c as u64 + 1);
    }
    hashes.push(mix(h));
    for i in k..codes.len() {
        h = h.wrapping_sub((codes[i - k] as u64 + 1).wrapping_mul(pow));
        h = h.wrapping_mul(BASE).wrapping_add(codes[i] as u64 + 1);
        hashes.push(mix(h));
    }
    hashes
}

/// Robust winnowing: in each window of `w` consecutive k-gram hashes select
/// the minimum, breaking ties by taking the rightmost occurrence, and record
/// a fingerprint only when the selected position changes between windows.
/// Returns the distinct fingerprint set, sorted.
pub fn winnow(codes: &[u16], k: usize, w: usize) -> Vec<u64> {
    let hashes = kgram_hashes(codes, k);
    if hashes.is_empty() {
        return Vec::new();
    }
    let mut fps: Vec<u64> = Vec::new();
    if hashes.len() <= w {
        // Fewer hashes than one full window: fingerprint the global minimum
        // so short-but-valid functions still get one fingerprint.
        let mut best = 0;
        for i in 0..hashes.len() {
            if hashes[i] <= hashes[best] {
                best = i;
            }
        }
        fps.push(hashes[best]);
    } else {
        let mut prev: Option<usize> = None;
        for start in 0..=hashes.len() - w {
            let mut best = start;
            for i in start..start + w {
                if hashes[i] <= hashes[best] {
                    best = i; // <= keeps the rightmost minimal hash
                }
            }
            if prev != Some(best) {
                fps.push(hashes[best]);
                prev = Some(best);
            }
        }
    }
    fps.sort_unstable();
    fps.dedup();
    fps
}

/// Jaccard similarity of two sorted, deduplicated fingerprint sets.
pub fn jaccard(a: &[u64], b: &[u64]) -> f64 {
    let shared = intersection_size(a, b);
    let union = a.len() + b.len() - shared;
    if union == 0 { 0.0 } else { shared as f64 / union as f64 }
}

pub fn intersection_size(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{KGRAM, WINDOW};

    fn pseudo_random_tokens(seed: u64, len: usize) -> Vec<u16> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = mix(state);
                (state % 97) as u16
            })
            .collect()
    }

    #[test]
    fn identical_streams_have_identical_fingerprints() {
        let toks = pseudo_random_tokens(7, 300);
        assert_eq!(winnow(&toks, KGRAM, WINDOW), winnow(&toks, KGRAM, WINDOW));
    }

    #[test]
    fn winnowing_guarantee_any_shared_run_of_t_tokens_is_caught() {
        // Plant the same run of exactly t = w + k - 1 tokens inside two
        // otherwise unrelated streams, at many alignments.
        let t = WINDOW + KGRAM - 1;
        let run = pseudo_random_tokens(42, t);
        for offset_a in [0usize, 3, 50, 117] {
            for offset_b in [0usize, 9, 80, 200] {
                let mut a = pseudo_random_tokens(1, 260);
                let mut b = pseudo_random_tokens(2, 260);
                a.splice(offset_a..offset_a + t, run.iter().copied());
                b.splice(offset_b..offset_b + t, run.iter().copied());
                let fa = winnow(&a, KGRAM, WINDOW);
                let fb = winnow(&b, KGRAM, WINDOW);
                assert!(
                    intersection_size(&fa, &fb) >= 1,
                    "guarantee violated at offsets {offset_a}/{offset_b}"
                );
            }
        }
    }

    #[test]
    fn no_match_shorter_than_k_is_possible() {
        // Streams sharing only runs of k-1 tokens share no k-gram, hence
        // cannot share a fingerprint.
        let shared: Vec<u16> = (0..KGRAM as u16 - 1).collect();
        let mut a = vec![900u16; 40];
        let mut b = vec![901u16; 40];
        a.extend(&shared);
        a.extend(vec![900u16; 40]);
        b.extend(&shared);
        b.extend(vec![901u16; 40]);
        let fa = winnow(&a, KGRAM, WINDOW);
        let fb = winnow(&b, KGRAM, WINDOW);
        assert_eq!(intersection_size(&fa, &fb), 0);
    }

    #[test]
    fn density_is_near_two_over_w_plus_one() {
        let toks = pseudo_random_tokens(99, 20_000);
        let fps = kgram_hashes(&toks, KGRAM).len();
        let selected = winnow(&toks, KGRAM, WINDOW).len();
        let density = selected as f64 / fps as f64;
        let expected = 2.0 / (WINDOW as f64 + 1.0);
        assert!(
            (density - expected).abs() < 0.05,
            "density {density:.3}, expected ~{expected:.3}"
        );
    }

    #[test]
    fn jaccard_basics() {
        assert_eq!(jaccard(&[1, 2, 3], &[1, 2, 3]), 1.0);
        assert_eq!(jaccard(&[1, 2], &[3, 4]), 0.0);
        assert!((jaccard(&[1, 2, 3], &[2, 3, 4]) - 0.5).abs() < 1e-9);
    }
}
