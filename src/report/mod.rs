pub mod card;
pub mod terminal;

use crate::cluster::Cluster;
use crate::extract::FunctionUnit;
use crate::score::Score;
use serde::Serialize;

pub const JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub root: String,
    pub stats: Stats,
    pub score: ScoreOut,
    pub clusters: Vec<ClusterOut>,
    /// Experimental class-shape clusters (`--include-classes`). Omitted from
    /// JSON when empty so the default output is byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub class_shapes: Vec<ClusterOut>,
}

#[derive(Serialize)]
pub struct Stats {
    pub files: u32,
    pub total_lines: u64,
    pub significant_lines: u64,
    pub functions: u32,
    pub elapsed_ms: u64,
    pub skipped_generated: u32,
    pub skipped_minified: u32,
    pub skipped_non_utf8: u32,
}

#[derive(Serialize)]
pub struct ScoreOut {
    /// % of significant lines deletable if every duplicate cluster kept
    /// only one copy.
    pub slop_percent: f64,
    pub grade: char,
    pub deletable_lines: u64,
}

#[derive(Serialize)]
pub struct ClusterOut {
    pub id: usize,
    pub copies: usize,
    /// Mean similarity of the non-representative members to the rep.
    pub similarity: f64,
    pub deletable_lines: u32,
    pub test_only: bool,
    /// Rust trait-impl look-alikes (`From`/`Display`/...), kept out of the
    /// slop score. Omitted when false so existing output is unchanged.
    #[serde(default, skip_serializing_if = "is_false")]
    pub trait_impl_only: bool,
    pub members: Vec<MemberOut>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize)]
pub struct MemberOut {
    pub file: String,
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub lines: u32,
    pub similarity: f64,
    pub representative: bool,
    pub test: bool,
}

/// Map internal clusters to their serializable form. Shared by the function
/// clusters and the experimental class-shape clusters.
pub fn clusters_out(
    clusters: &[Cluster],
    functions: &[FunctionUnit],
    file_names: &[String],
) -> Vec<ClusterOut> {
    clusters
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut members: Vec<MemberOut> = c
                .members
                .iter()
                .map(|m| {
                    let f = &functions[m.func as usize];
                    MemberOut {
                        file: file_names[f.file as usize].clone(),
                        name: f.name.clone(),
                        start_line: f.start_line,
                        end_line: f.end_line,
                        lines: f.sig_lines,
                        similarity: m.similarity,
                        representative: false,
                        test: f.is_test,
                    }
                })
                .collect();
            if let Some(first) = members.first_mut() {
                first.representative = true;
            }
            let non_rep: Vec<f64> = c.members[1..].iter().map(|m| m.similarity).collect();
            let similarity = if non_rep.is_empty() {
                1.0
            } else {
                non_rep.iter().sum::<f64>() / non_rep.len() as f64
            };
            ClusterOut {
                id: i + 1,
                copies: c.members.len(),
                similarity,
                deletable_lines: c.deletable_lines,
                test_only: c.test_only,
                trait_impl_only: c.trait_impl_only,
                members,
            }
        })
        .collect()
}

pub fn build(
    root: &str,
    file_names: &[String],
    functions: &[FunctionUnit],
    clusters: &[Cluster],
    score: &Score,
    stats: Stats,
) -> Report {
    // Under TestPolicy::Skip test files were never scanned; under FlagOnly
    // test clusters are shown (labeled) but excluded from the score.
    let clusters_out = clusters_out(clusters, functions, file_names);

    Report {
        schema_version: JSON_SCHEMA_VERSION,
        root: root.to_string(),
        stats,
        score: ScoreOut {
            slop_percent: score.percent,
            grade: score.grade,
            deletable_lines: score.deletable_lines,
        },
        clusters: clusters_out,
        class_shapes: Vec::new(),
    }
}
