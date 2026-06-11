//! The slop score: the percentage of your code you could delete if every
//! duplicate cluster kept only one copy. That one sentence is the whole
//! formula — the representative copy is exempt, test-only clusters are
//! excluded by default, and nothing here guesses who (or what) wrote the
//! code.

use crate::cluster::Cluster;
use crate::config::TestPolicy;

pub struct Score {
    /// 0.0 - 100.0
    pub percent: f64,
    pub grade: char,
    pub deletable_lines: u64,
    pub total_sig_lines: u64,
}

pub fn slop_score(clusters: &[Cluster], total_sig_lines: u64, tests: TestPolicy) -> Score {
    let deletable: u64 = clusters
        .iter()
        .filter(|c| tests == TestPolicy::Include || !c.test_only)
        .map(|c| c.deletable_lines as u64)
        .sum();
    let percent = if total_sig_lines == 0 {
        0.0
    } else {
        100.0 * deletable as f64 / total_sig_lines as f64
    };
    Score {
        percent,
        grade: grade(percent),
        deletable_lines: deletable,
        total_sig_lines,
    }
}

/// Grade buckets, calibrated against well-maintained OSS repos (see the
/// calibration table in the README).
pub fn grade(percent: f64) -> char {
    match percent {
        p if p < 3.0 => 'A',
        p if p < 6.0 => 'B',
        p if p < 10.0 => 'C',
        p if p < 15.0 => 'D',
        _ => 'F',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cluster(deletable: u32, test_only: bool) -> Cluster {
        Cluster {
            members: vec![],
            deletable_lines: deletable,
            test_only,
        }
    }

    #[test]
    fn score_excludes_test_clusters_by_default() {
        let clusters = vec![cluster(50, false), cluster(100, true)];
        let s = slop_score(&clusters, 1000, TestPolicy::FlagOnly);
        assert!((s.percent - 5.0).abs() < 1e-9);
        assert_eq!(s.grade, 'B');

        let s = slop_score(&clusters, 1000, TestPolicy::Include);
        assert!((s.percent - 15.0).abs() < 1e-9);
        assert_eq!(s.grade, 'F');
    }

    #[test]
    fn empty_repo_scores_zero() {
        let s = slop_score(&[], 0, TestPolicy::FlagOnly);
        assert_eq!(s.percent, 0.0);
        assert_eq!(s.grade, 'A');
    }
}
