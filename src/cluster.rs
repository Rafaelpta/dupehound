//! Groups matching pairs into duplicate clusters with union-find, picks a
//! representative per cluster, and suppresses nested clusters (an inner
//! closure duplicated because its whole enclosing function is duplicated).

use crate::extract::FunctionUnit;
use crate::index::{Pair, similarity};

pub struct Member {
    pub func: u32,
    /// Similarity to the cluster representative (1.0 for the rep itself).
    pub similarity: f64,
}

pub struct Cluster {
    /// Representative (most significant lines) first.
    pub members: Vec<Member>,
    /// Lines you could delete by keeping only the representative.
    pub deletable_lines: u32,
    /// True when every member is test code.
    pub test_only: bool,
    /// True when every member is a Rust trait-impl method (`From`, `Display`,
    /// ...). Like `test_only`, these are kept out of the slop score: each impl
    /// is a distinct, required implementation, not deletable duplication.
    pub trait_impl_only: bool,
}

struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n as u32).collect(),
        }
    }
    fn find(&mut self, x: u32) -> u32 {
        let mut root = x;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        let mut cur = x;
        while self.parent[cur as usize] != root {
            let next = self.parent[cur as usize];
            self.parent[cur as usize] = root;
            cur = next;
        }
        root
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra as usize] = rb;
        }
    }
}

pub fn build_clusters(functions: &[FunctionUnit], pairs: &[Pair]) -> Vec<Cluster> {
    let mut uf = UnionFind::new(functions.len());
    for p in pairs {
        uf.union(p.a, p.b);
    }

    let mut groups: rustc_hash::FxHashMap<u32, Vec<u32>> = rustc_hash::FxHashMap::default();
    for p in pairs {
        let root = uf.find(p.a);
        let g = groups.entry(root).or_default();
        for id in [p.a, p.b] {
            if !g.contains(&id) {
                g.push(id);
            }
        }
    }

    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .map(|mut ids| {
            // Representative = the member with the most significant lines;
            // the "original" is exempt from the deletable count.
            ids.sort_by_key(|&id| {
                let f = &functions[id as usize];
                (std::cmp::Reverse(f.sig_lines), f.file, f.start_line)
            });
            let rep = ids[0];
            let members: Vec<Member> = ids
                .iter()
                .map(|&id| Member {
                    func: id,
                    similarity: if id == rep {
                        1.0
                    } else {
                        similarity(&functions[id as usize], &functions[rep as usize])
                    },
                })
                .collect();
            let deletable_lines = ids[1..]
                .iter()
                .map(|&id| functions[id as usize].sig_lines)
                .sum();
            let test_only = ids.iter().all(|&id| functions[id as usize].is_test);
            let trait_impl_only = ids
                .iter()
                .all(|&id| functions[id as usize].is_trait_impl_method);
            Cluster {
                members,
                deletable_lines,
                test_only,
                trait_impl_only,
            }
        })
        .collect();

    suppress_nested(functions, &mut clusters);
    clusters.sort_by_key(|c| std::cmp::Reverse(c.deletable_lines));
    clusters
}

/// Drop a cluster when each of its members sits inside a member of one other
/// cluster — that's an inner function matching only because its enclosing
/// function is itself duplicated, and counting both double-counts the lines.
fn suppress_nested(functions: &[FunctionUnit], clusters: &mut Vec<Cluster>) {
    let contains = |outer: &FunctionUnit, inner: &FunctionUnit| {
        outer.file == inner.file
            && outer.start_byte <= inner.start_byte
            && inner.end_byte <= outer.end_byte
            && (outer.start_byte, outer.end_byte) != (inner.start_byte, inner.end_byte)
    };
    let snapshot: Vec<Vec<u32>> = clusters
        .iter()
        .map(|c| c.members.iter().map(|m| m.func).collect())
        .collect();
    let mut keep = vec![true; clusters.len()];
    for (i, members) in snapshot.iter().enumerate() {
        'outer_cluster: for (j, outer_members) in snapshot.iter().enumerate() {
            if i == j {
                continue;
            }
            for &m in members {
                let inner = &functions[m as usize];
                let nested = outer_members
                    .iter()
                    .any(|&o| contains(&functions[o as usize], inner));
                if !nested {
                    continue 'outer_cluster;
                }
            }
            keep[i] = false;
            break;
        }
    }
    let mut idx = 0;
    clusters.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;

    fn unit(
        file: u32,
        lines: (u32, u32),
        bytes: (u32, u32),
        sig: u32,
        fps: Vec<u64>,
    ) -> FunctionUnit {
        FunctionUnit {
            file,
            lang: Lang::Typescript,
            name: "f".into(),
            start_line: lines.0,
            end_line: lines.1,
            start_byte: bytes.0,
            end_byte: bytes.1,
            sig_lines: sig,
            fingerprints: fps,
            is_test: false,
            is_trait_impl_method: false,
        }
    }

    #[test]
    fn pairs_merge_into_one_cluster_with_largest_as_rep() {
        let functions = vec![
            unit(0, (1, 10), (0, 100), 9, vec![1, 2, 3]),
            unit(1, (1, 12), (0, 120), 11, vec![1, 2, 3]),
            unit(2, (1, 10), (0, 100), 9, vec![1, 2, 3]),
        ];
        let pairs = vec![Pair { a: 0, b: 1 }, Pair { a: 1, b: 2 }];
        let clusters = build_clusters(&functions, &pairs);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members.len(), 3);
        assert_eq!(clusters[0].members[0].func, 1); // largest is rep
        assert_eq!(clusters[0].deletable_lines, 18);
    }

    #[test]
    fn nested_cluster_is_suppressed() {
        // Outer functions 0 and 1 duplicate each other; inner functions
        // 2-in-0 and 3-in-1 also "duplicate" — but only because their
        // parents do.
        let functions = vec![
            unit(0, (1, 30), (0, 900), 28, vec![1, 2, 3, 4]),
            unit(1, (1, 30), (0, 900), 28, vec![1, 2, 3, 4]),
            unit(0, (5, 12), (100, 400), 7, vec![2, 3]),
            unit(1, (5, 12), (100, 400), 7, vec![2, 3]),
        ];
        let pairs = vec![Pair { a: 0, b: 1 }, Pair { a: 2, b: 3 }];
        let clusters = build_clusters(&functions, &pairs);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members.len(), 2);
        assert_eq!(clusters[0].deletable_lines, 28);
    }
}
