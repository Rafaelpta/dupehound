use crate::cli::CommonArgs;

/// Winnowing parameters. k-gram size and window size give the guarantee
/// threshold t = w + k - 1: any shared run of >= t normalized tokens is
/// always caught by at least one shared fingerprint.
pub const KGRAM: usize = 10;
pub const WINDOW: usize = 8;

pub const DEFAULT_SCAN_THRESHOLD: f64 = 0.80;
pub const DEFAULT_CHECK_THRESHOLD: f64 = 0.85;

/// Minimum raw shared fingerprints before we bother computing exact Jaccard.
pub const MIN_SHARED_PREFILTER: u32 = 3;

/// Files larger than this are skipped (almost certainly generated/minified).
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TestPolicy {
    /// Scan test files, flag them, exclude test-only clusters from the score.
    FlagOnly,
    /// Treat test files like any other code.
    Include,
    /// Don't scan test files at all.
    Skip,
}

#[derive(Clone)]
pub struct Config {
    pub threshold: f64,
    pub min_tokens: usize,
    pub excludes: Vec<String>,
    pub default_excludes: bool,
    pub tests: TestPolicy,
    pub json: bool,
}

impl Config {
    pub fn from_common(common: &CommonArgs, default_threshold: f64) -> Self {
        let tests = if common.include_tests {
            TestPolicy::Include
        } else if common.exclude_tests {
            TestPolicy::Skip
        } else {
            TestPolicy::FlagOnly
        };
        Config {
            threshold: common
                .threshold
                .unwrap_or(default_threshold)
                .clamp(0.0, 1.0),
            min_tokens: common.min_tokens,
            excludes: common.excludes.clone(),
            default_excludes: !common.no_default_excludes,
            tests,
            json: common.json,
        }
    }
}
