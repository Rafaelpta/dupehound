#![cfg(feature = "card")]

//! The shareable card: a 1200×630 (OG-image sized) SVG, rasterized to PNG
//! at 2×. Fonts are embedded in the binary so the card renders identically
//! on every machine.

use super::Report;
use crate::history::HistoryReport;
use anyhow::{Context, Result};
use std::path::Path;

const FONT_REGULAR: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");
const FONT_XBOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-ExtraBold.ttf");

const W: f64 = 1200.0;
const H: f64 = 630.0;

const BG: &str = "#0d1117";
const PANEL: &str = "#161b22";
const BORDER: &str = "#30363d";
const TEXT: &str = "#e6edf3";
const DIM: &str = "#8b949e";
const GREEN: &str = "#3fb950";
const YELLOW: &str = "#d29922";
const RED: &str = "#f85149";

fn grade_color(grade: char) -> &'static str {
    match grade {
        'A' | 'B' => GREEN,
        'C' => YELLOW,
        _ => RED,
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn frame(body: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
<rect width="{W}" height="{H}" fill="{BG}"/>
<rect x="1.5" y="1.5" width="{w2}" height="{h2}" rx="14" fill="none" stroke="{BORDER}" stroke-width="3"/>
{body}
<text x="60" y="588" font-family="JetBrains Mono" font-size="20" fill="{DIM}">dupehound — sniffs out the code your AI wrote twice · fast · offline · no AI required</text>
</svg>"##,
        w2 = W - 3.0,
        h2 = H - 3.0,
    )
}

/// Card for `scan --card`: the score, huge.
pub fn score_card(report: &Report) -> String {
    let color = grade_color(report.score.grade);
    let repo = esc(&report.root);
    let pct = format!("{:.1}%", report.score.slop_percent);
    let body = format!(
        r##"<text x="60" y="96" font-family="JetBrains Mono" font-weight="bold" font-size="34" fill="{TEXT}">{repo}</text>
<text x="60" y="146" font-family="JetBrains Mono" font-size="24" fill="{DIM}">SLOP SCORE</text>
<text x="54" y="340" font-family="JetBrains Mono" font-weight="800" font-size="170" fill="{color}">{pct}</text>
<rect x="60" y="386" width="150" height="64" rx="10" fill="{PANEL}" stroke="{BORDER}"/>
<text x="135" y="430" font-family="JetBrains Mono" font-weight="bold" font-size="32" fill="{color}" text-anchor="middle">grade {grade}</text>
<text x="60" y="500" font-family="JetBrains Mono" font-size="24" fill="{DIM}">{deletable} of {total} significant lines are deletable duplicates</text>
"##,
        grade = report.score.grade,
        deletable = group(report.score.deletable_lines),
        total = group(report.stats.significant_lines),
    );
    frame(&body)
}

/// Card for `history`: the area chart with the inflection callout.
pub fn history_card(r: &HistoryReport) -> String {
    let current = r.series.last().expect("non-empty series");
    let color = grade_color(r.current_grade);
    let repo = esc(&r.repo);
    let pct = format!("{:.1}%", current.slop_percent);

    // Chart geometry.
    let (cx, cy, cw, ch) = (60.0, 250.0, 1080.0, 260.0);
    let max = r
        .series
        .iter()
        .map(|p| p.slop_percent)
        .fold(0.0f64, f64::max)
        .max(0.1)
        * 1.15;
    let pts: Vec<(f64, f64)> = r
        .series
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = cx + cw * i as f64 / (r.series.len() - 1).max(1) as f64;
            let y = cy + ch - ch * (p.slop_percent / max);
            (x, y)
        })
        .collect();
    let line = smooth_path(&pts);
    let area = format!(
        "{line} L {:.1} {:.1} L {:.1} {:.1} Z",
        pts.last().unwrap().0,
        cy + ch,
        pts[0].0,
        cy + ch
    );

    // Inflection marker: the snapshot where growth was measured from.
    let marker = r
        .growth_since
        .as_ref()
        .and_then(|since| r.series.iter().position(|p| &p.date == since))
        .map(|i| {
            let (x, y) = pts[i];
            format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="7" fill="{BG}" stroke="{color}" stroke-width="3"/>
<line x1="{x:.1}" y1="{y:.1}" x2="{x:.1}" y2="{top}" stroke="{color}" stroke-width="2" stroke-dasharray="4 5" opacity="0.7"/>"##,
                top = cy - 8.0,
            )
        })
        .unwrap_or_default();

    let headline = match (&r.growth_factor, &r.growth_since) {
        (Some(f), Some(since)) => format!("duplication grew {f:.1}× since {since}"),
        (None, Some(since)) => format!(
            "duplication went from ~0 to {:.1}% since {since}",
            current.slop_percent
        ),
        _ => "no significant duplication growth".to_string(),
    };

    let body = format!(
        r##"<text x="60" y="96" font-family="JetBrains Mono" font-weight="bold" font-size="34" fill="{TEXT}">{repo}</text>
<text x="60" y="142" font-family="JetBrains Mono" font-size="22" fill="{DIM}">{from} → {to} · {n} snapshots</text>
<text x="1140" y="96" font-family="JetBrains Mono" font-weight="800" font-size="64" fill="{color}" text-anchor="end">{pct}</text>
<text x="1140" y="138" font-family="JetBrains Mono" font-size="24" fill="{DIM}" text-anchor="end">slop score · grade {grade}</text>
<linearGradient id="fill" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0%" stop-color="{color}" stop-opacity="0.35"/>
  <stop offset="100%" stop-color="{color}" stop-opacity="0.02"/>
</linearGradient>
<line x1="{cx}" y1="{base}" x2="{cx2}" y2="{base}" stroke="{BORDER}" stroke-width="2"/>
<path d="{area}" fill="url(#fill)"/>
<path d="{line}" fill="none" stroke="{color}" stroke-width="4" stroke-linecap="round"/>
{marker}
<text x="60" y="556" font-family="JetBrains Mono" font-weight="bold" font-size="30" fill="{TEXT}">{headline}</text>
"##,
        from = r.series[0].date,
        to = current.date,
        n = r.series.len(),
        grade = r.current_grade,
        base = cy + ch,
        cx2 = cx + cw,
    );
    frame(&body)
}

/// Catmull-Rom spline through the points, emitted as cubic beziers.
fn smooth_path(pts: &[(f64, f64)]) -> String {
    if pts.len() < 2 {
        return String::new();
    }
    let mut d = format!("M {:.1} {:.1}", pts[0].0, pts[0].1);
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < pts.len() { pts[i + 2] } else { p2 };
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        d.push_str(&format!(
            " C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }
    d
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Write the SVG and a 2× PNG next to each other.
pub fn write_card(svg: &str, dir: &Path) -> Result<()> {
    std::fs::write(dir.join("dupehound-card.svg"), svg)?;

    let mut opt = resvg::usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        db.load_font_data(FONT_REGULAR.to_vec());
        db.load_font_data(FONT_BOLD.to_vec());
        db.load_font_data(FONT_XBOLD.to_vec());
    }
    let tree = resvg::usvg::Tree::from_str(svg, &opt).context("parsing card svg")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new((W * 2.0) as u32, (H * 2.0) as u32)
        .context("allocating pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pixmap.as_mut(),
    );
    pixmap
        .save_png(dir.join("dupehound-card.png"))
        .context("writing card png")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Point;

    fn fake_history() -> HistoryReport {
        let series: Vec<Point> = (0..18)
            .map(|i| Point {
                commit: format!("c{i:08}"),
                date: format!("20{:02}-{:02}", 24 + i / 12, 1 + i % 12),
                slop_percent: (i as f64).powf(1.6) / 4.0,
                significant_lines: 10_000 + i as u64 * 500,
                functions: 400 + i as u32 * 13,
            })
            .collect();
        HistoryReport {
            schema_version: 1,
            repo: "acme-api".into(),
            growth_factor: Some(4.1),
            growth_since: Some(series[6].date.clone()),
            current_grade: 'D',
            series,
        }
    }

    #[test]
    fn history_card_renders_svg_and_png() {
        let dir = tempfile::tempdir().unwrap();
        let svg = history_card(&fake_history());
        assert!(svg.contains("acme-api"));
        assert!(svg.contains("4.1×"));
        write_card(&svg, dir.path()).unwrap();
        let png = std::fs::metadata(dir.path().join("dupehound-card.png")).unwrap();
        assert!(png.len() > 20_000, "png suspiciously small: {}", png.len());
        let svg_file = std::fs::metadata(dir.path().join("dupehound-card.svg")).unwrap();
        assert!(svg_file.len() > 1_000);
    }

    #[test]
    fn png_render_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let svg = history_card(&fake_history());
        write_card(&svg, dir.path()).unwrap();
        let first = std::fs::read(dir.path().join("dupehound-card.png")).unwrap();
        write_card(&svg, dir.path()).unwrap();
        let second = std::fs::read(dir.path().join("dupehound-card.png")).unwrap();
        assert_eq!(
            first, second,
            "embedded fonts should make renders identical"
        );
    }
}
