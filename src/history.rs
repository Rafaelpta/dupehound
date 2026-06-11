//! The `history` command: re-measure duplication at sampled commits across
//! the repo's life — without ever checking anything out — and chart it.
//!
//! Fast because of the blob cache: month to month most file blobs are
//! identical, so each distinct blob is parsed and fingerprinted exactly
//! once across the whole run.

use crate::cli::HistoryArgs;
use crate::cluster::build_clusters;
use crate::config::{Config, DEFAULT_SCAN_THRESHOLD, MAX_FILE_BYTES, MIN_SHARED_PREFILTER};
use crate::extract::{FunctionUnit, analyze_source};
use crate::gitsnap::{BlobReader, git, ls_tree, repo_root};
use crate::index::find_pairs;
use crate::lang::Lang;
use crate::score::{grade, slop_score};
use crate::walk::{content_skip_reason, default_excluded, is_test_path};
use anyhow::Result;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Point {
    pub commit: String,
    pub date: String, // YYYY-MM
    pub slop_percent: f64,
    pub significant_lines: u64,
    pub functions: u32,
}

#[derive(Serialize)]
pub struct HistoryReport {
    pub schema_version: u32,
    pub repo: String,
    pub series: Vec<Point>,
    /// "duplication grew Nx since DATE", when the data supports it.
    pub growth_factor: Option<f64>,
    pub growth_since: Option<String>,
    pub current_grade: char,
}

pub fn run(args: HistoryArgs) -> Result<i32> {
    let config = Config::from_common(&args.common, DEFAULT_SCAN_THRESHOLD);
    let repo = repo_root(&args.path)?;
    let snapshots = pick_snapshots(&repo, args.max_snapshots)?;
    if snapshots.len() < 2 {
        eprintln!(
            "dupehound history: not enough history to chart ({} usable snapshot{})",
            snapshots.len(),
            if snapshots.len() == 1 { "" } else { "s" }
        );
        return Ok(2);
    }

    let mut cache: BlobCache = FxHashMap::default();
    let mut reader = BlobReader::open(&repo)?;
    let mut series = Vec::with_capacity(snapshots.len());
    for (commit, month) in &snapshots {
        let point = measure(&repo, commit, month, &config, &mut cache, &mut reader)?;
        series.push(point);
    }

    let (growth_factor, growth_since) = inflection(&series);
    let current_grade = grade(series.last().map(|p| p.slop_percent).unwrap_or(0.0));
    let report = HistoryReport {
        schema_version: crate::report::JSON_SCHEMA_VERSION,
        repo: repo_name(&repo),
        series,
        growth_factor,
        growth_since,
        current_grade,
    };

    if config.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&report));
    }

    #[cfg(feature = "card")]
    if !args.no_card {
        let svg = crate::report::card::history_card(&report);
        crate::report::card::write_card(&svg, std::path::Path::new("."))?;
        eprintln!("  card written → dupehound-card.svg / dupehound-card.png");
    }

    Ok(0)
}

/// blob oid -> analysis (None = skipped file).
type BlobCache = FxHashMap<String, Option<CachedFile>>;

#[derive(Clone)]
struct CachedFile {
    functions: Vec<FunctionUnit>,
    sig_lines: u32,
}

fn measure(
    repo: &std::path::Path,
    commit: &str,
    month: &str,
    config: &Config,
    cache: &mut BlobCache,
    reader: &mut BlobReader,
) -> Result<Point> {
    let entries: Vec<_> = ls_tree(repo, commit)?
        .into_iter()
        .filter(|e| {
            Lang::from_path(&e.path).is_some()
                && !(config.default_excludes && default_excluded(&e.path))
        })
        .collect();

    // Read blobs we haven't fingerprinted yet (sequential IO, parallel CPU).
    let missing: Vec<&crate::gitsnap::TreeEntry> = entries
        .iter()
        .filter(|e| !cache.contains_key(&e.oid))
        .collect();
    let mut fresh: Vec<(String, String, Vec<u8>)> = Vec::with_capacity(missing.len());
    for e in missing {
        if let Some(bytes) = reader.read_blob(&e.oid)? {
            if bytes.len() as u64 <= MAX_FILE_BYTES {
                fresh.push((e.oid.clone(), e.path.clone(), bytes));
            } else {
                cache.insert(e.oid.clone(), None);
            }
        } else {
            cache.insert(e.oid.clone(), None);
        }
    }
    let analyzed: Vec<(String, Option<CachedFile>)> = fresh
        .par_iter()
        .map(|(oid, path, bytes)| {
            let analysis = String::from_utf8(bytes.clone()).ok().and_then(|src| {
                if content_skip_reason(&src).is_some() {
                    return None;
                }
                let lang = Lang::from_path(path)?;
                let fa = analyze_source(0, lang, &src, config.min_tokens, is_test_path(path))?;
                Some(CachedFile {
                    functions: fa.functions,
                    sig_lines: fa.sig_lines,
                })
            });
            (oid.clone(), analysis)
        })
        .collect();
    cache.extend(analyzed);

    // Assemble this snapshot from the cache.
    let mut functions: Vec<FunctionUnit> = Vec::new();
    let mut sig_lines = 0u64;
    let mut file_id = 0u32;
    for e in &entries {
        let Some(Some(cached)) = cache.get(&e.oid) else {
            continue;
        };
        sig_lines += cached.sig_lines as u64;
        let is_test = is_test_path(&e.path);
        for f in &cached.functions {
            let mut f = f.clone();
            f.file = file_id;
            f.is_test = f.is_test || is_test;
            functions.push(f);
        }
        file_id += 1;
    }

    let function_count = functions.len() as u32;
    let pairs = find_pairs(&mut functions, config.threshold, MIN_SHARED_PREFILTER);
    let clusters = build_clusters(&functions, &pairs);
    let score = slop_score(&clusters, sig_lines, config.tests);

    Ok(Point {
        commit: commit[..commit.len().min(10)].to_string(),
        date: month.to_string(),
        slop_percent: score.percent,
        significant_lines: sig_lines,
        functions: function_count,
    })
}

/// Mainline commits bucketed by month: the last commit of each month, down-
/// sampled evenly to `max`, always ending at HEAD.
fn pick_snapshots(repo: &std::path::Path, max: usize) -> Result<Vec<(String, String)>> {
    let log = git(
        repo,
        &["log", "--first-parent", "--reverse", "--format=%H %ct", "HEAD"],
    )?;
    let mut by_month: Vec<(String, String)> = Vec::new(); // (commit, YYYY-MM)
    for line in log.lines() {
        let Some((hash, ts)) = line.split_once(' ') else {
            continue;
        };
        let Ok(ts) = ts.trim().parse::<i64>() else {
            continue;
        };
        let month = month_of(ts);
        match by_month.last_mut() {
            Some(last) if last.1 == month => last.0 = hash.to_string(),
            _ => by_month.push((hash.to_string(), month)),
        }
    }
    if by_month.is_empty() {
        return Ok(vec![]);
    }
    // Young repo: fall back to evenly spaced commits instead of months.
    if by_month.len() < 6 {
        let all: Vec<(String, String)> = log
            .lines()
            .filter_map(|l| {
                let (hash, ts) = l.split_once(' ')?;
                let ts: i64 = ts.trim().parse().ok()?;
                Some((hash.to_string(), month_of(ts)))
            })
            .collect();
        return Ok(evenly_spaced(&all, max.min(12)));
    }
    Ok(evenly_spaced(&by_month, max))
}

fn evenly_spaced(items: &[(String, String)], max: usize) -> Vec<(String, String)> {
    if items.len() <= max {
        return items.to_vec();
    }
    let mut picked = Vec::with_capacity(max);
    for i in 0..max {
        let idx = i * (items.len() - 1) / (max - 1);
        picked.push(items[idx].clone());
    }
    picked.dedup_by(|a, b| a.0 == b.0);
    picked
}

/// Unix timestamp -> "YYYY-MM" (civil-from-days, Howard Hinnant's algorithm).
fn month_of(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}")
}

/// Headline growth: current slop vs the minimum of the 3-point smoothed
/// series. Honest when flat: under 1.5x we report nothing.
fn inflection(series: &[Point]) -> (Option<f64>, Option<String>) {
    if series.len() < 3 {
        return (None, None);
    }
    let smoothed: Vec<f64> = (0..series.len())
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(series.len() - 1);
            let window = &series[lo..=hi];
            window.iter().map(|p| p.slop_percent).sum::<f64>() / window.len() as f64
        })
        .collect();
    let current = *smoothed.last().unwrap();
    let (min_idx, min_val) = smoothed
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, v)| (i, *v))
        .unwrap();
    if min_val <= 0.05 {
        // Effectively from zero: a ratio is meaningless, so report the end
        // of the clean era instead (rendered as "went from ~0 to X%").
        if current >= 1.0 {
            // Raw series, not smoothed: smoothing bleeds the first dirty
            // month backwards and would misdate the clean era's end.
            let last_clean = series
                .iter()
                .rposition(|p| p.slop_percent <= 0.05)
                .unwrap_or(0);
            if last_clean < series.len() - 1 {
                return (None, Some(series[last_clean].date.clone()));
            }
        }
        return (None, None);
    }
    let factor = current / min_val;
    if factor >= 1.5 && min_idx < series.len() - 1 {
        (Some(factor), Some(series[min_idx].date.clone()))
    } else {
        (None, None)
    }
}

fn repo_name(repo: &std::path::Path) -> String {
    repo.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.display().to_string())
}

const CHART_HEIGHT: usize = 7;

fn render(r: &HistoryReport) -> String {
    use owo_colors::{OwoColorize, Stream::Stdout};
    let dim = |s: &str| s.if_supports_color(Stdout, |t| t.dimmed()).to_string();
    let bold = |s: &str| s.if_supports_color(Stdout, |t| t.bold()).to_string();

    let mut out = String::new();
    let current = r.series.last().unwrap();
    out.push('\n');
    out.push_str(&dim(&format!(
        "  dupehound history — {} · {} snapshots · {} → {}",
        r.repo,
        r.series.len(),
        r.series[0].date,
        current.date,
    )));
    out.push_str("\n\n");

    // Chart
    let max = r
        .series
        .iter()
        .map(|p| p.slop_percent)
        .fold(0.0f64, f64::max)
        .max(0.1);
    let cols: Vec<usize> = r
        .series
        .iter()
        .map(|p| ((p.slop_percent / max) * (CHART_HEIGHT * 8) as f64).round() as usize)
        .collect();
    for row in (0..CHART_HEIGHT).rev() {
        let label = if row == CHART_HEIGHT - 1 {
            format!("{max:>5.1}% ")
        } else if row == 0 {
            format!("{:>5.1}% ", 0.0)
        } else {
            "       ".to_string()
        };
        out.push_str(&dim(&format!("  {label}┤")));
        for &c in &cols {
            let cell = c.saturating_sub(row * 8).min(8);
            let ch = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"][cell];
            out.push_str(ch);
            out.push_str(ch); // two chars per snapshot for visibility
        }
        out.push('\n');
    }
    out.push_str(&dim(&format!(
        "         └{}\n",
        "─".repeat(r.series.len() * 2)
    )));
    let axis_width = (r.series.len() * 2).saturating_sub(7);
    out.push_str(&dim(&format!(
        "          {}{:>axis_width$}\n",
        r.series[0].date, current.date,
    )));
    out.push('\n');

    out.push_str(&format!(
        "  current slop score: {} (grade {})\n",
        bold(&format!("{:.1}%", current.slop_percent)),
        grade(current.slop_percent),
    ));
    out.push_str(&match (&r.growth_factor, &r.growth_since) {
        (Some(factor), Some(since)) => format!(
            "  {} {}\n",
            bold(&format!("duplication grew {factor:.1}× since {since}")),
            dim("(vs the smoothed minimum)"),
        ),
        (None, Some(since)) => format!(
            "  {}\n",
            bold(&format!(
                "duplication went from ~0 to {:.1}% since {since}",
                current.slop_percent
            )),
        ),
        _ => dim("  no significant duplication growth detected\n"),
    });
    out.push('\n');
    out
}
