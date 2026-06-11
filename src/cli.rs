use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "dupehound",
    version,
    about = "Sniffs out near-duplicate code. Fast, offline, no AI required.",
    long_about = "dupehound finds near-duplicate functions across your codebase — even when \
identifiers and literals were renamed. It fingerprints normalized syntax using the winnowing \
algorithm (Schleimer, Wilkerson & Aiken, SIGMOD 2003) and never sends code anywhere."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan a directory for duplicate functions and compute the slop score
    Scan(ScanArgs),
    /// Chart duplication over git history and find the inflection point
    History(HistoryArgs),
    /// CI gate: fail when newly added code duplicates existing code
    Check(CheckArgs),
}

#[derive(Args)]
pub struct CommonArgs {
    /// Minimum similarity (0.0-1.0) for two functions to count as duplicates
    #[arg(long)]
    pub threshold: Option<f64>,

    /// Ignore functions with fewer normalized tokens than this
    #[arg(long, default_value_t = 40)]
    pub min_tokens: usize,

    /// Extra glob patterns to exclude (repeatable)
    #[arg(long = "exclude", value_name = "GLOB")]
    pub excludes: Vec<String>,

    /// Don't apply the built-in exclusions (vendor/, dist/, generated files, ...)
    #[arg(long)]
    pub no_default_excludes: bool,

    /// Include test files in the slop score (they are excluded by default)
    #[arg(long)]
    pub include_tests: bool,

    /// Skip test files entirely (default: scanned but excluded from the score)
    #[arg(long, conflicts_with = "include_tests")]
    pub exclude_tests: bool,

    /// Emit machine-readable JSON instead of the terminal report
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct ScanArgs {
    /// Directory to scan
    #[arg(default_value = ".")]
    pub path: PathBuf,

    #[command(flatten)]
    pub common: CommonArgs,

    /// Show every cluster instead of the top 10
    #[arg(long)]
    pub all: bool,

    /// Print the code of cluster N side by side as proof
    #[arg(long, value_name = "CLUSTER")]
    pub explain: Option<usize>,

    /// Also write a shareable score card (dupehound-card.svg/.png)
    #[arg(long)]
    pub card: bool,
}

#[derive(Args)]
pub struct HistoryArgs {
    /// Git repository to analyze
    #[arg(default_value = ".")]
    pub path: PathBuf,

    #[command(flatten)]
    pub common: CommonArgs,

    /// Maximum number of historical snapshots to measure
    #[arg(long, default_value_t = 36)]
    pub max_snapshots: usize,

    /// Skip writing the shareable card
    #[arg(long)]
    pub no_card: bool,
}

#[derive(Args)]
pub struct CheckArgs {
    /// Git repository to check
    #[arg(default_value = ".")]
    pub path: PathBuf,

    #[command(flatten)]
    pub common: CommonArgs,

    /// Compare against the merge-base with this revision (PR semantics)
    #[arg(long, value_name = "REV")]
    pub diff: Option<String>,
}
