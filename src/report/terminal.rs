//! The default terminal report. One-shot output, degrades to plain ASCII
//! when piped or when NO_COLOR is set (owo-colors' supports-colors feature
//! handles detection).

use super::{ClusterOut, Report};
use owo_colors::{OwoColorize, Stream::Stdout};

const TOP_N: usize = 10;

fn dim(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

fn bold(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.bold()).to_string()
}

fn yellow(s: &str) -> String {
    s.if_supports_color(Stdout, |t| t.yellow()).to_string()
}

fn grade_color(s: &str, grade: char) -> String {
    match grade {
        'A' | 'B' => s
            .if_supports_color(Stdout, |t| t.bright_green().bold().to_string())
            .to_string(),
        'C' => s
            .if_supports_color(Stdout, |t| t.yellow().bold().to_string())
            .to_string(),
        _ => s
            .if_supports_color(Stdout, |t| t.bright_red().bold().to_string())
            .to_string(),
    }
}

pub fn render(report: &Report, all: bool) -> String {
    let mut out = String::new();
    header(&mut out, report);
    score_box(&mut out, report);
    clusters(&mut out, report, all);
    footer(&mut out, report);
    out
}

fn header(out: &mut String, r: &Report) {
    out.push('\n');
    out.push_str(&dim(&format!(
        "  dupehound v{} — scanned {} files · {} lines · {} functions in {}",
        env!("CARGO_PKG_VERSION"),
        group_digits(r.stats.files as u64),
        group_digits(r.stats.total_lines),
        group_digits(r.stats.functions as u64),
        human_ms(r.stats.elapsed_ms),
    )));
    out.push_str("\n\n");
}

fn score_box(out: &mut String, r: &Report) {
    let pct = format!("{:.1}%", r.score.slop_percent);
    let grade = format!("grade {}", r.score.grade);
    let plain1 = format!("SLOP SCORE   {pct}   {grade}");
    let line2 = format!(
        "{} of {} significant lines are deletable duplicates",
        group_digits(r.score.deletable_lines),
        group_digits(r.stats.significant_lines),
    );
    let inner = line2.len().max(plain1.len()) + 4;

    let line1 = format!(
        "{}   {}   {}",
        bold("SLOP SCORE"),
        grade_color(&pct, r.score.grade),
        dim(&grade)
    );

    out.push_str(&format!("  ╭{}╮\n", "─".repeat(inner)));
    out.push_str(&format!(
        "  │  {}{}│\n",
        line1,
        " ".repeat(inner.saturating_sub(plain1.len() + 2))
    ));
    out.push_str(&format!(
        "  │  {}{}│\n",
        dim(&line2),
        " ".repeat(inner.saturating_sub(line2.len() + 2))
    ));
    out.push_str(&format!("  ╰{}╯\n\n", "─".repeat(inner)));
}

fn clusters(out: &mut String, r: &Report, all: bool) {
    if r.clusters.is_empty() {
        out.push_str("  No duplicate clusters found. Good dog. 🐕\n\n");
        return;
    }
    let shown = if all {
        r.clusters.len()
    } else {
        r.clusters.len().min(TOP_N)
    };
    let hidden = r.clusters.len() - shown;
    out.push_str(&dim(&format!(
        "  {} duplicate cluster{} · showing top {} by deletable lines{}",
        r.clusters.len(),
        if r.clusters.len() == 1 { "" } else { "s" },
        shown,
        if hidden > 0 {
            "  (--all for everything)"
        } else {
            ""
        },
    )));
    out.push_str("\n\n");

    for c in &r.clusters[..shown] {
        render_cluster(out, c);
    }

    if hidden > 0 {
        let hidden_lines: u64 = r.clusters[shown..]
            .iter()
            .map(|c| c.deletable_lines as u64)
            .sum();
        out.push_str(&dim(&format!(
            "  … {} more clusters ({} deletable lines) — run with --all\n",
            hidden,
            group_digits(hidden_lines)
        )));
        out.push('\n');
    }
}

fn render_cluster(out: &mut String, c: &ClusterOut) {
    let test_tag = if c.test_only {
        " · [tests, not scored]"
    } else {
        ""
    };
    let title = format!(
        "● Cluster {} ─ {} copies · {:.0}% similar · {} deletable lines{}",
        c.id,
        c.copies,
        c.similarity * 100.0,
        group_digits(c.deletable_lines as u64),
        test_tag,
    );
    let pad = 78usize.saturating_sub(visible_len(&title) + 3);
    out.push_str(&format!("  {} {}\n", bold(&title), dim(&"─".repeat(pad))));

    let loc_width = c
        .members
        .iter()
        .map(|m| format!("{}:{}", m.file, m.start_line).len())
        .max()
        .unwrap_or(0)
        .min(58);
    let name_width = c
        .members
        .iter()
        .map(|m| m.name.len())
        .max()
        .unwrap_or(0)
        .min(28);

    for m in &c.members {
        let loc = truncate_left(&format!("{}:{}", m.file, m.start_line), loc_width);
        let name = truncate(&m.name, name_width);
        let lines = format!("{} lines", m.lines);
        if m.representative {
            out.push_str(&format!(
                "    {} {loc:<loc_width$}  {name:<name_width$}  {lines:>9}\n",
                yellow("★"),
            ));
        } else {
            let pct = format!("{:>3.0}%", m.similarity * 100.0);
            out.push_str(&format!(
                "      {loc:<loc_width$}  {name:<name_width$}  {lines:>9}   {} {}\n",
                bold(&pct),
                dim(&sim_bar(m.similarity)),
            ));
        }
    }
    out.push('\n');
}

fn footer(out: &mut String, r: &Report) {
    let mut notes: Vec<String> = Vec::new();
    if r.stats.skipped_generated > 0 {
        notes.push(format!("{} generated", r.stats.skipped_generated));
    }
    if r.stats.skipped_minified > 0 {
        notes.push(format!("{} minified", r.stats.skipped_minified));
    }
    if r.stats.skipped_non_utf8 > 0 {
        notes.push(format!("{} non-utf8", r.stats.skipped_non_utf8));
    }
    if !notes.is_empty() {
        out.push_str(&dim(&format!("  skipped files: {}\n", notes.join(" · "))));
    }
    out.push_str(&dim(
        "  slop score = lines deletable if every duplicate cluster kept one copy\n",
    ));
    if !r.clusters.is_empty() {
        out.push_str(&dim(
            "  ★ = representative (kept) · dupehound scan --explain 1 shows the code\n",
        ));
    }
}

/// A 9-cell bar built from eighth-block characters.
fn sim_bar(similarity: f64) -> String {
    let eighths = (similarity.clamp(0.0, 1.0) * 72.0).round() as usize;
    let full = eighths / 8;
    let rem = eighths % 8;
    let blocks = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let mut bar = "█".repeat(full.min(9));
    if full < 9 && rem > 0 {
        bar.push_str(blocks[rem]);
    }
    bar
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn human_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().skip(n - max + 1).collect();
        format!("…{cut}")
    }
}

fn visible_len(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}
