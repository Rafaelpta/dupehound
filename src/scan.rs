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
        print!("{}", explain(&output, cluster_id, args.full)?);
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

const SIDE_BY_SIDE_MIN_WIDTH: usize = 120;
const DIFF_CONTEXT: usize = 3;

/// A text-styling function (one of the `style::*` color helpers).
type StyleFn = fn(&str) -> String;

/// Show how the copies of a cluster differ from the representative.
///
/// By default this is a colored diff: each copy against the representative, with
/// unchanged runs collapsed and changed tokens emphasized. On a TTY it is
/// side-by-side when the terminal is wide and unified otherwise; piped, it is a
/// plain unified diff that stays grep- and copy-paste-friendly. `--full` prints
/// every body in full instead (numbered and wrapped on a TTY).
fn explain(output: &ScanOutput, cluster_id: usize, full: bool) -> Result<String> {
    use crate::style;

    let Some(cluster) = output.report.clusters.iter().find(|c| c.id == cluster_id) else {
        anyhow::bail!(
            "no cluster {} (there are {})",
            cluster_id,
            output.report.clusters.len()
        );
    };
    let internal = &output.clusters[cluster_id - 1];
    let pretty = style::is_tty();
    let width = style::term_width().unwrap_or(100);

    let mut out = String::new();
    out.push_str(&format!(
        "\n  {}\n\n",
        style::bold(&format!(
            "Cluster {} · {} copies · {:.0}% similar",
            cluster.id,
            cluster.copies,
            cluster.similarity * 100.0
        ))
    ));

    // Read a member's function body from disk by its byte range. Tabs are
    // expanded so the diff's column math matches what the terminal renders.
    let read = |i: usize| -> Option<String> {
        let f = &output.functions[internal.members[i].func as usize];
        let path = &output.file_paths[f.file as usize];
        let content = std::fs::read_to_string(path).ok()?;
        Some(content[f.start_byte as usize..f.end_byte as usize].replace('\t', "    "))
    };

    if full {
        render_full(&mut out, output, internal, &cluster.members, pretty, width);
        return Ok(out);
    }

    let rep_i = cluster
        .members
        .iter()
        .position(|m| m.representative)
        .unwrap_or(0);
    let rep = &cluster.members[rep_i];
    let rep_src = read(rep_i).unwrap_or_default();
    let rep_loc = format!("{}:{}", rep.file, rep.start_line);
    let rep_loc = if pretty {
        // "  ── {loc} {name}  ★ representative" = loc + name + 24 fixed columns.
        style::truncate_left(
            &rep_loc,
            width.saturating_sub(rep.name.chars().count() + 24).max(16),
        )
    } else {
        rep_loc
    };
    out.push_str(&format!(
        "  {} {} {}  {}\n\n",
        style::dim("──"),
        style::dim(&rep_loc),
        style::bold(&rep.name),
        style::green("★ representative")
    ));

    for (i, m) in cluster.members.iter().enumerate() {
        if i == rep_i {
            continue;
        }
        let copy_src = read(i).unwrap_or_default();
        let loc = format!("{}:{}", m.file, m.start_line);
        let loc = if pretty {
            style::truncate_left(&loc, width.saturating_sub(20).max(16))
        } else {
            loc
        };
        out.push_str(&format!(
            "  {} {}  {}\n",
            style::dim("vs"),
            style::dim(&loc),
            style::yellow(&format!("{:.0}% similar", m.similarity * 100.0))
        ));
        if copy_src == rep_src {
            out.push_str(&format!(
                "     {}\n\n",
                style::dim("identical to representative")
            ));
            continue;
        }
        if pretty && width >= SIDE_BY_SIDE_MIN_WIDTH {
            render_diff_side_by_side(
                &mut out,
                &rep_src,
                &copy_src,
                rep.start_line,
                m.start_line,
                width,
            );
        } else {
            render_diff_unified(
                &mut out,
                &rep_src,
                &copy_src,
                rep.start_line,
                m.start_line,
                width,
            );
        }
        out.push('\n');
    }
    Ok(out)
}

/// `--full`: print each member's whole body, numbered and wrapped on a TTY,
/// plain `│ ` gutter when piped.
fn render_full(
    out: &mut String,
    output: &ScanOutput,
    internal: &Cluster,
    members: &[crate::report::MemberOut],
    pretty: bool,
    width: usize,
) {
    use crate::style;
    for (m_out, m) in members.iter().zip(&internal.members) {
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
                render_body(out, snippet, f.start_line, f.end_line, width);
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
}

/// Widest line number either side of the diff will print.
fn line_num_width(old_start: u32, old: &str, new_start: u32, new: &str) -> usize {
    let oe = old_start as usize + old.lines().count().saturating_sub(1);
    let ne = new_start as usize + new.lines().count().saturating_sub(1);
    oe.max(ne).max(1).to_string().len()
}

/// Visible width of a string in terminal columns.
fn vis_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Collect a line's inline segments, dropping the trailing newline.
fn inline_segments<'a>(
    it: impl Iterator<Item = (bool, std::borrow::Cow<'a, str>)>,
) -> Vec<(bool, String)> {
    it.map(|(em, v)| (em, v.trim_end_matches(['\n', '\r']).to_string()))
        .collect()
}

/// Style a line's segments, truncating to `max_cols` columns with a dim `…`.
/// `base` styles ordinary text, `emph` the changed tokens. Returns the styled
/// string and the visible columns it occupies.
fn styled_truncated(
    segs: &[(bool, String)],
    base: StyleFn,
    emph: StyleFn,
    max_cols: usize,
) -> (String, usize) {
    use unicode_width::UnicodeWidthChar;
    let total: usize = segs.iter().map(|(_, v)| vis_width(v)).sum();
    let paint = |em: bool, t: &str| if em { emph(t) } else { base(t) };
    if total <= max_cols {
        let mut out = String::new();
        for (em, v) in segs {
            if !v.is_empty() {
                out.push_str(&paint(*em, v));
            }
        }
        return (out, total);
    }
    let cap = max_cols.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for (em, v) in segs {
        if used >= cap {
            break;
        }
        let mut buf = String::new();
        for ch in v.chars() {
            let cw = ch.width().unwrap_or(0);
            if used + cw > cap {
                break;
            }
            buf.push(ch);
            used += cw;
        }
        if !buf.is_empty() {
            out.push_str(&paint(*em, &buf));
        }
    }
    out.push_str(&crate::style::dim("…"));
    (out, used + 1)
}

/// Unified diff: equal context dim, deletions `-` red, insertions `+` green,
/// changed tokens emphasized, unchanged runs collapsed. Long lines truncate.
fn render_diff_unified(
    out: &mut String,
    old: &str,
    new: &str,
    old_start: u32,
    new_start: u32,
    width: usize,
) {
    use crate::style;
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(DIFF_CONTEXT);
    let num_w = line_num_width(old_start, old, new_start, new);
    let budget = width.saturating_sub(num_w + 5).max(8);

    let mut prev_end = 0usize;
    for (gi, group) in groups.iter().enumerate() {
        let gap = group[0].old_range().start.saturating_sub(prev_end);
        if gi > 0 && gap > 0 {
            out.push_str(&format!(
                "  {}\n",
                style::dim(&format!("⋮ {gap} unchanged lines"))
            ));
        }
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let (sign, lineno, base, emph): (&str, u32, StyleFn, StyleFn) = match change.tag() {
                    ChangeTag::Equal => (
                        " ",
                        old_start + change.old_index().unwrap_or(0) as u32,
                        style::dim,
                        style::dim,
                    ),
                    ChangeTag::Delete => (
                        "-",
                        old_start + change.old_index().unwrap_or(0) as u32,
                        style::red,
                        style::red_emph,
                    ),
                    ChangeTag::Insert => (
                        "+",
                        new_start + change.new_index().unwrap_or(0) as u32,
                        style::green,
                        style::green_emph,
                    ),
                };
                let segs = inline_segments(change.iter_strings_lossy());
                let (body, _) = styled_truncated(&segs, base, emph, budget);
                out.push_str(&format!(
                    "  {} {} {}\n",
                    style::dim(&format!("{lineno:>num_w$}")),
                    base(sign),
                    body
                ));
            }
        }
        prev_end = group[group.len() - 1].old_range().end;
    }
}

/// Side-by-side diff: representative on the left, the copy on the right, each in
/// its own numbered column truncated to half the width. Changed tokens are
/// emphasized; unchanged runs collapse.
fn render_diff_side_by_side(
    out: &mut String,
    old: &str,
    new: &str,
    old_start: u32,
    new_start: u32,
    width: usize,
) {
    use crate::style;
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(DIFF_CONTEXT);
    let num_w = line_num_width(old_start, old, new_start, new);
    // Row = "  " + (num + " " + col) + " " + (num + " " + col) = 2*num_w + 2*col + 5.
    let col = (width.saturating_sub(2 * num_w + 5) / 2).max(8);

    // One rendered half-row: a line number (or blanks) and styled, padded text.
    let half = |lineno: Option<u32>, body: &str, used: usize| -> String {
        let num = match lineno {
            Some(n) => style::dim(&format!("{n:>num_w$}")),
            None => " ".repeat(num_w),
        };
        let pad = " ".repeat(col.saturating_sub(used));
        format!("{num} {body}{pad}")
    };
    let blank_half = || format!("{} {}", " ".repeat(num_w), " ".repeat(col));

    let mut prev_end = 0usize;
    for (gi, group) in groups.iter().enumerate() {
        let gap = group[0].old_range().start.saturating_sub(prev_end);
        if gi > 0 && gap > 0 {
            out.push_str(&format!(
                "  {}\n",
                style::dim(&format!("⋮ {gap} unchanged lines"))
            ));
        }
        // Buffer deletions/insertions so a changed block pairs old↔new row-wise.
        let mut dels: Vec<(u32, String, usize)> = Vec::new();
        let mut inss: Vec<(u32, String, usize)> = Vec::new();
        let flush = |out: &mut String,
                     dels: &mut Vec<(u32, String, usize)>,
                     inss: &mut Vec<(u32, String, usize)>| {
            for i in 0..dels.len().max(inss.len()) {
                let left = match dels.get(i) {
                    Some((n, b, u)) => half(Some(*n), b, *u),
                    None => blank_half(),
                };
                let right = match inss.get(i) {
                    Some((n, b, u)) => half(Some(*n), b, *u),
                    None => blank_half(),
                };
                out.push_str(&format!("  {left} {right}\n"));
            }
            dels.clear();
            inss.clear();
        };

        for op in group {
            for change in diff.iter_inline_changes(op) {
                let segs = inline_segments(change.iter_strings_lossy());
                match change.tag() {
                    ChangeTag::Equal => {
                        flush(out, &mut dels, &mut inss);
                        let (b, u) = styled_truncated(&segs, style::dim, style::dim, col);
                        let oln = old_start + change.old_index().unwrap_or(0) as u32;
                        let nln = new_start + change.new_index().unwrap_or(0) as u32;
                        out.push_str(&format!(
                            "  {} {}\n",
                            half(Some(oln), &b, u),
                            half(Some(nln), &b, u)
                        ));
                    }
                    ChangeTag::Delete => {
                        let (b, u) = styled_truncated(&segs, style::red, style::red_emph, col);
                        dels.push((old_start + change.old_index().unwrap_or(0) as u32, b, u));
                    }
                    ChangeTag::Insert => {
                        let (b, u) = styled_truncated(&segs, style::green, style::green_emph, col);
                        inss.push((new_start + change.new_index().unwrap_or(0) as u32, b, u));
                    }
                }
            }
        }
        flush(out, &mut dels, &mut inss);
        prev_end = group[group.len() - 1].old_range().end;
    }
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
