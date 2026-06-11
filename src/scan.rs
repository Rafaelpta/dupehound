//! The `scan` command: discover files, fingerprint every function, find
//! duplicate clusters, score, report.

use crate::cli::ScanArgs;
use crate::cluster::{Cluster, build_clusters};
use crate::config::{Config, DEFAULT_SCAN_THRESHOLD, MIN_SHARED_PREFILTER, TestPolicy};
use crate::extract::{FunctionUnit, analyze_source};
use crate::index::find_pairs;
use crate::report::{Report, Stats, build, terminal};
use crate::score::slop_score;
use crate::walk::{DiscoveredFile, discover, load};
use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

pub struct ScanOutput {
    pub report: Report,
    pub functions: Vec<FunctionUnit>,
    pub clusters: Vec<Cluster>,
    pub file_paths: Vec<std::path::PathBuf>,
}

pub fn run(args: ScanArgs) -> Result<i32> {
    let config = Config::from_common(&args.common, DEFAULT_SCAN_THRESHOLD);
    let output = scan_path(&args.path, &config)?;

    if let Some(cluster_id) = args.explain {
        print!("{}", explain(&output, cluster_id)?);
        return Ok(0);
    }

    if config.json {
        println!("{}", serde_json::to_string_pretty(&output.report)?);
    } else {
        print!("{}", terminal::render(&output.report, args.all));
    }

    #[cfg(feature = "card")]
    if args.card {
        let svg = crate::report::card::score_card(&output.report);
        crate::report::card::write_card(&svg, Path::new("."))?;
        eprintln!("  card written → dupehound-card.svg / dupehound-card.png");
    }

    Ok(0)
}

/// Shared scan pipeline (also used by `history` on worktree-less snapshots
/// and by `check` for the base index).
pub fn scan_path(root: &Path, config: &Config) -> Result<ScanOutput> {
    let started = Instant::now();
    let mut discovered = discover(root, config)?;
    if config.tests == TestPolicy::Skip {
        discovered.retain(|f| !f.is_test);
    }

    let skipped_generated = AtomicU32::new(0);
    let skipped_minified = AtomicU32::new(0);
    let skipped_non_utf8 = AtomicU32::new(0);

    struct FileResult {
        rel: String,
        path: std::path::PathBuf,
        functions: Vec<FunctionUnit>,
        sig_lines: u32,
        total_lines: u32,
    }

    let mut results: Vec<FileResult> = discovered
        .par_iter()
        .filter_map(|f: &DiscoveredFile| {
            let content = match load(&f.path) {
                Ok(Ok(content)) => content,
                Ok(Err("generated")) => {
                    skipped_generated.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                Ok(Err("minified")) => {
                    skipped_minified.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                Ok(Err(_)) => {
                    skipped_non_utf8.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                Err(_) => return None,
            };
            // File id is stamped after the parallel phase (stable order).
            let fa = analyze_source(0, f.lang, &content, config.min_tokens, f.is_test)?;
            Some(FileResult {
                rel: f.rel.clone(),
                path: f.path.clone(),
                functions: fa.functions,
                sig_lines: fa.sig_lines,
                total_lines: fa.total_lines,
            })
        })
        .collect();
    results.sort_by(|a, b| a.rel.cmp(&b.rel));

    let mut file_names = Vec::with_capacity(results.len());
    let mut file_paths = Vec::with_capacity(results.len());
    let mut functions: Vec<FunctionUnit> = Vec::new();
    let mut total_lines = 0u64;
    let mut sig_lines = 0u64;
    for (i, mut r) in results.into_iter().enumerate() {
        for f in &mut r.functions {
            f.file = i as u32;
        }
        functions.extend(r.functions);
        file_names.push(r.rel);
        file_paths.push(r.path);
        total_lines += r.total_lines as u64;
        sig_lines += r.sig_lines as u64;
    }

    let pairs = find_pairs(&mut functions, config.threshold, MIN_SHARED_PREFILTER);
    let clusters = build_clusters(&functions, &pairs);
    let score = slop_score(&clusters, sig_lines, config.tests);

    let stats = Stats {
        files: file_names.len() as u32,
        total_lines,
        significant_lines: sig_lines,
        functions: functions.len() as u32,
        elapsed_ms: started.elapsed().as_millis() as u64,
        skipped_generated: skipped_generated.into_inner(),
        skipped_minified: skipped_minified.into_inner(),
        skipped_non_utf8: skipped_non_utf8.into_inner(),
    };

    let report = build(
        &root.display().to_string(),
        &file_names,
        &functions,
        &clusters,
        &score,
        stats,
    );

    Ok(ScanOutput {
        report,
        functions,
        clusters,
        file_paths,
    })
}

/// Print the representative and each copy of a cluster, in full, as proof.
fn explain(output: &ScanOutput, cluster_id: usize) -> Result<String> {
    let Some(cluster) = output.report.clusters.iter().find(|c| c.id == cluster_id) else {
        anyhow::bail!(
            "no cluster {} (there are {})",
            cluster_id,
            output.report.clusters.len()
        );
    };
    let internal = &output.clusters[cluster_id - 1];
    let mut out = String::new();
    out.push_str(&format!(
        "\n  Cluster {} — {} copies, {:.0}% similar\n\n",
        cluster.id,
        cluster.copies,
        cluster.similarity * 100.0
    ));
    for (m_out, m) in cluster.members.iter().zip(&internal.members) {
        let f = &output.functions[m.func as usize];
        let marker = if m_out.representative {
            "★ representative"
        } else {
            "duplicate"
        };
        out.push_str(&format!(
            "  ── {}:{} {} ({}) {}\n",
            m_out.file,
            m_out.start_line,
            m_out.name,
            marker,
            if m_out.representative {
                String::new()
            } else {
                format!("— {:.0}% similar", m_out.similarity * 100.0)
            }
        ));
        let path = &output.file_paths[f.file as usize];
        if let Ok(content) = std::fs::read_to_string(path) {
            let snippet = &content[f.start_byte as usize..f.end_byte as usize];
            for line in snippet.lines() {
                out.push_str("  │ ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    Ok(out)
}
