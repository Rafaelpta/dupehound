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
///
/// On a TTY the body is rendered with a numbered, dimmed gutter and long lines
/// soft-wrap under a `↳` continuation so nothing overflows the terminal. When
/// piped (no width available) it falls back to a plain `│ ` gutter that stays
/// copy-paste and grep friendly.
fn explain(output: &ScanOutput, cluster_id: usize) -> Result<String> {
    use crate::style;

    let Some(cluster) = output.report.clusters.iter().find(|c| c.id == cluster_id) else {
        anyhow::bail!(
            "no cluster {} (there are {})",
            cluster_id,
            output.report.clusters.len()
        );
    };
    let internal = &output.clusters[cluster_id - 1];
    // A TTY gets the pretty rendering; anything piped stays plain. Width comes
    // from the terminal, falling back to 100 when it can't be read.
    let pretty = style::is_tty();
    let width = style::term_width().unwrap_or(100);
    let mut out = String::new();

    let header = format!(
        "Cluster {} · {} copies · {:.0}% similar",
        cluster.id,
        cluster.copies,
        cluster.similarity * 100.0
    );
    out.push_str(&format!("\n  {}\n\n", style::bold(&header)));

    for (m_out, m) in cluster.members.iter().zip(&internal.members) {
        let f = &output.functions[m.func as usize];
        let marker_plain = if m_out.representative {
            "★ representative".to_string()
        } else {
            format!("{:.0}% similar", m_out.similarity * 100.0)
        };
        let marker = if m_out.representative {
            style::green(&marker_plain)
        } else {
            style::yellow(&marker_plain)
        };
        let mut loc = format!("{}:{}", m_out.file, m_out.start_line);
        if pretty {
            // header is "  ── {loc} {name}  {marker}": 8 fixed columns plus the
            // three variable parts. Truncate loc so the whole line fits width.
            let budget = width
                .saturating_sub(8 + m_out.name.chars().count() + marker_plain.chars().count())
                .max(12);
            loc = style::truncate_left(&loc, budget);
        }
        out.push_str(&format!(
            "  {} {} {}  {}\n",
            style::dim("──"),
            style::dim(&loc),
            style::bold(&m_out.name),
            marker
        ));

        let path = &output.file_paths[f.file as usize];
        if let Ok(content) = std::fs::read_to_string(path) {
            let snippet = &content[f.start_byte as usize..f.end_byte as usize];
            if pretty {
                render_body(&mut out, snippet, f.start_line, f.end_line, width);
            } else {
                for line in snippet.lines() {
                    out.push_str("  │ ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out.push('\n');
    }
    Ok(out)
}

/// Render a function body for a TTY: a right-aligned line number, a dimmed
/// `│` gutter, and soft-wrapping so a long line never runs past `width`. The
/// wrapped remainder is indented under a dimmed `↳`.
fn render_body(out: &mut String, snippet: &str, start_line: u32, end_line: u32, width: usize) {
    use crate::style;

    let num_w = end_line.to_string().len();
    // "  " indent + number + " │ " on the first row; the continuation row swaps
    // the number for blanks and the leading space-pipe for "│ ↳ ".
    let first_budget = width.saturating_sub(2 + num_w + 3).max(8);
    let cont_budget = width.saturating_sub(2 + num_w + 5).max(8);

    for (i, raw) in snippet.lines().enumerate() {
        let lineno = start_line + i as u32;
        let line = expand_tabs(raw, 4);
        let segs = wrap_to_width(&line, first_budget, cont_budget);

        let gutter = style::dim(&format!("{lineno:>num_w$} │ "));
        out.push_str("  ");
        out.push_str(&gutter);
        out.push_str(segs.first().map_or("", |s| s.as_str()));
        out.push('\n');

        for seg in segs.iter().skip(1) {
            let cont = style::dim(&format!("{:>num_w$} │ ↳ ", ""));
            out.push_str("  ");
            out.push_str(&cont);
            out.push_str(seg);
            out.push('\n');
        }
    }
}

/// Expand tab characters to `n` spaces so wrapping and indentation line up.
fn expand_tabs(s: &str, n: usize) -> String {
    if s.contains('\t') {
        s.replace('\t', &" ".repeat(n))
    } else {
        s.to_string()
    }
}

/// Split a line into display-width-bounded segments: the first capped at
/// `first`, the rest at `cont`. Wraps on character boundaries (code, not prose),
/// measuring with Unicode display width.
fn wrap_to_width(s: &str, first: usize, cont: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;

    let mut segs = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    let mut budget = first;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if cur_w + cw > budget && !cur.is_empty() {
            segs.push(std::mem::take(&mut cur));
            cur_w = 0;
            budget = cont;
        }
        cur.push(ch);
        cur_w += cw;
    }
    if !cur.is_empty() || segs.is_empty() {
        segs.push(cur);
    }
    segs
}
