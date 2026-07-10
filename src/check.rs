//! The `check` command: a CI / pre-commit gate. Newly added or modified
//! functions are probed against an index of the codebase as it existed at
//! the base revision; a hit means "this logic already exists — reuse it".
//!
//! Exit codes: 0 clean · 1 duplicates found · 2 error.

use crate::cli::CheckArgs;
use crate::config::{Config, DEFAULT_CHECK_THRESHOLD, MAX_FILE_BYTES};
use crate::extract::{FunctionUnit, analyze_source};
use crate::fingerprint::jaccard;
use crate::gitsnap::{BlobReader, git, ls_tree, repo_root};
use crate::lang::Lang;
use crate::walk::{content_skip_reason, default_excluded, is_test_path};
use anyhow::Result;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

/// A function in this changeset only fires when it overlaps a changed hunk
/// AND duplicates base code this strongly.
const MOVE_EQUIVALENCE: f64 = 0.90;

enum Mode {
    /// Compare merge-base(rev, HEAD)..HEAD — PR semantics for CI.
    Range { base: String },
    /// Compare the staged index against HEAD — pre-commit semantics.
    Staged,
    /// Compare the working tree against HEAD.
    Worktree,
}

#[derive(Serialize)]
pub struct Finding {
    pub file: String,
    pub line: u32,
    pub name: String,
    pub similarity: f64,
    pub original_file: String,
    pub original_line: u32,
    pub original_name: String,
    /// The import/use line that brings the original into scope, when the
    /// language has one derivable from the path. Omitted from JSON when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// The result of a check: whether the changeset touched any code at all, and
/// the duplicate findings. The CLI and the MCP server share this; only the CLI
/// turns it into printed output.
pub struct CheckOutcome {
    pub had_changes: bool,
    pub findings: Vec<Finding>,
}

pub fn compute(args: &CheckArgs) -> Result<CheckOutcome> {
    let config = Config::from_common(&args.common, DEFAULT_CHECK_THRESHOLD);
    let repo = repo_root(&args.path)?;

    let mode = if let Some(rev) = &args.diff {
        let base = git(&repo, &["merge-base", rev, "HEAD"])?.trim().to_string();
        Mode::Range { base }
    } else {
        let staged = git(&repo, &["diff", "--cached", "--name-only"])?;
        if staged.trim().is_empty() {
            Mode::Worktree
        } else {
            Mode::Staged
        }
    };
    let base_rev = match &mode {
        Mode::Range { base } => base.clone(),
        Mode::Staged | Mode::Worktree => "HEAD".to_string(),
    };

    // What changed, and which new-side lines.
    let diff = match &mode {
        Mode::Range { base } => git(
            &repo,
            &["diff", "-U0", "--no-color", "--no-ext-diff", base, "HEAD"],
        )?,
        Mode::Staged => git(
            &repo,
            &["diff", "-U0", "--no-color", "--no-ext-diff", "--cached"],
        )?,
        Mode::Worktree => git(
            &repo,
            &["diff", "-U0", "--no-color", "--no-ext-diff", "HEAD"],
        )?,
    };
    let mut changed = parse_hunks(&diff);
    // `git diff HEAD` won't list brand-new untracked files — the most
    // common way a duplicate arrives. Treat them as fully changed.
    if matches!(mode, Mode::Worktree) {
        let untracked = git(&repo, &["ls-files", "--others", "--exclude-standard"])?;
        for path in untracked.lines().filter(|l| !l.is_empty()) {
            changed
                .entry(path.to_string())
                .or_default()
                .push((1, u32::MAX));
        }
    }
    if changed.is_empty() {
        return Ok(CheckOutcome {
            had_changes: false,
            findings: Vec::new(),
        });
    }

    // Index every function as of the base revision.
    let (base_functions, base_files) = index_revision(&repo, &base_rev, &config)?;
    let mut by_fp: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
    for (i, f) in base_functions.iter().enumerate() {
        for &fp in &f.fingerprints {
            by_fp.entry(fp).or_default().push(i as u32);
        }
    }

    let changed_paths: FxHashSet<&str> = changed.keys().map(|s| s.as_str()).collect();

    // Extract candidate functions from the new side of each changed file.
    let mut findings: Vec<Finding> = Vec::new();
    let mut new_side_cache: FxHashMap<String, Vec<FunctionUnit>> = FxHashMap::default();
    for (path, hunks) in &changed {
        let Some(_lang) = Lang::from_path(path) else {
            continue;
        };
        if default_excluded(path) || is_test_path(path) {
            continue;
        }
        let functions = new_side_functions(&repo, &mode, path, &config, &mut new_side_cache)?;
        for func in functions {
            let overlaps = hunks
                .iter()
                .any(|&(start, end)| func.start_line <= end && start <= func.end_line);
            if !overlaps {
                continue;
            }
            // Probe the base index.
            let mut shared: FxHashMap<u32, u32> = FxHashMap::default();
            for fp in &func.fingerprints {
                if let Some(list) = by_fp.get(fp) {
                    for &id in list {
                        *shared.entry(id).or_insert(0) += 1;
                    }
                }
            }
            let mut best: Option<(u32, f64)> = None;
            for (id, count) in shared {
                let other = &base_functions[id as usize];
                let union = func.fingerprints.len() + other.fingerprints.len() - count as usize;
                let sim = if union == 0 {
                    0.0
                } else {
                    count as f64 / union as f64
                };
                if sim >= config.threshold && best.is_none_or(|(_, b)| sim > b) {
                    best = Some((id, sim));
                }
            }
            let Some((base_id, sim)) = best else { continue };
            let base_fn = &base_functions[base_id as usize];
            let base_path = &base_files[base_fn.file as usize];

            // Same function, edited: skip.
            if base_path == path && base_fn.name == func.name {
                continue;
            }
            // Moved function: the original's file changed too and the
            // original is gone from its new version.
            if changed_paths.contains(base_path.as_str()) {
                let originals =
                    new_side_functions(&repo, &mode, base_path, &config, &mut new_side_cache)?;
                let still_there = originals
                    .iter()
                    .any(|f| jaccard(&f.fingerprints, &base_fn.fingerprints) >= MOVE_EQUIVALENCE);
                if !still_there {
                    continue;
                }
            }
            let suggestion = Lang::from_path(base_path).and_then(|lang| {
                crate::suggest::import_suggestion(lang, base_path, &base_fn.name, path)
            });
            findings.push(Finding {
                file: path.clone(),
                line: func.start_line,
                name: func.name.clone(),
                similarity: sim,
                original_file: base_path.clone(),
                original_line: base_fn.start_line,
                original_name: base_fn.name.clone(),
                suggestion,
            });
        }
    }

    findings.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    Ok(CheckOutcome {
        had_changes: true,
        findings,
    })
}

/// CLI entry: compute findings, then print them. Exit codes: 0 clean or no
/// changes, 1 duplicates found, 2 error (the `?` bubbles to main).
pub fn run(args: CheckArgs) -> Result<i32> {
    let outcome = compute(&args)?;
    if !outcome.had_changes {
        println!("dupehound check: no changes to check");
        return Ok(0);
    }
    if args.common.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": crate::report::JSON_SCHEMA_VERSION,
                "findings": outcome.findings,
            }))?
        );
    } else if outcome.findings.is_empty() {
        println!("dupehound check: no new duplicates ✓");
    } else {
        for f in &outcome.findings {
            println!(
                "{}:{} {}() is a {:.0}% duplicate of {}:{} {}() — reuse it",
                f.file,
                f.line,
                f.name,
                f.similarity * 100.0,
                f.original_file,
                f.original_line,
                f.original_name,
            );
            if let Some(s) = &f.suggestion {
                println!("      {s}");
            }
        }
        eprintln!(
            "\ndupehound check: {} new duplicate{} of existing code",
            outcome.findings.len(),
            if outcome.findings.len() == 1 { "" } else { "s" }
        );
    }
    Ok(if outcome.findings.is_empty() { 0 } else { 1 })
}

/// New-side content of a changed file, depending on mode.
fn new_side_functions(
    repo: &std::path::Path,
    mode: &Mode,
    path: &str,
    config: &Config,
    cache: &mut FxHashMap<String, Vec<FunctionUnit>>,
) -> Result<Vec<FunctionUnit>> {
    if let Some(found) = cache.get(path) {
        return Ok(found.clone());
    }
    let content: Option<String> = match mode {
        Mode::Range { .. } => git(repo, &["show", &format!("HEAD:{path}")]).ok(),
        Mode::Staged => git(repo, &["show", &format!(":{path}")]).ok(),
        Mode::Worktree => std::fs::read_to_string(repo.join(path)).ok(),
    };
    let functions = match content {
        Some(src) if content_skip_reason(&src).is_none() => Lang::from_path(path)
            .and_then(|lang| analyze_source(0, lang, &src, config.min_tokens, false))
            .map(|fa| fa.functions)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    cache.insert(path.to_string(), functions.clone());
    Ok(functions)
}

/// Parse `git diff -U0` output into path -> new-side changed line ranges.
fn parse_hunks(diff: &str) -> FxHashMap<String, Vec<(u32, u32)>> {
    let mut result: FxHashMap<String, Vec<(u32, u32)>> = FxHashMap::default();
    let mut current: Option<String> = None;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current = Some(path.to_string());
        } else if line.starts_with("+++ /dev/null") {
            current = None; // deletion — nothing on the new side
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            let Some(path) = &current else { continue };
            // "@@ -a,b +c,d @@" — we want the +c,d part.
            let Some(plus) = rest.split(' ').find(|p| p.starts_with('+')) else {
                continue;
            };
            let nums = &plus[1..];
            let (start, len) = match nums.split_once(',') {
                Some((s, l)) => (s.parse().unwrap_or(0), l.parse().unwrap_or(0u32)),
                None => (nums.parse().unwrap_or(0), 1),
            };
            if len > 0 {
                result
                    .entry(path.clone())
                    .or_default()
                    .push((start, start + len - 1));
            }
        }
    }
    result
}

/// Fingerprint every function in the repo as of `rev`.
fn index_revision(
    repo: &std::path::Path,
    rev: &str,
    config: &Config,
) -> Result<(Vec<FunctionUnit>, Vec<String>)> {
    let entries: Vec<_> = ls_tree(repo, rev)?
        .into_iter()
        .filter(|e| {
            Lang::from_path(&e.path).is_some()
                && !(config.default_excludes && default_excluded(&e.path))
                && !is_test_path(&e.path)
        })
        .collect();

    // One cat-file process; blobs read sequentially, parsing parallelized.
    let mut reader = BlobReader::open(repo)?;
    let mut blobs: Vec<(String, Vec<u8>)> = Vec::with_capacity(entries.len());
    for e in &entries {
        if let Some(bytes) = reader.read_blob(&e.oid)? {
            if bytes.len() as u64 <= MAX_FILE_BYTES {
                blobs.push((e.path.clone(), bytes));
            }
        }
    }

    let analyzed: Vec<(String, Vec<FunctionUnit>)> = blobs
        .par_iter()
        .filter_map(|(path, bytes)| {
            let src = String::from_utf8(bytes.clone()).ok()?;
            if content_skip_reason(&src).is_some() {
                return None;
            }
            let lang = Lang::from_path(path)?;
            let fa = analyze_source(0, lang, &src, config.min_tokens, false)?;
            Some((path.clone(), fa.functions))
        })
        .collect();

    let mut functions = Vec::new();
    let mut files = Vec::new();
    for (path, mut funcs) in analyzed {
        let file_id = files.len() as u32;
        for f in &mut funcs {
            f.file = file_id;
        }
        functions.extend(funcs);
        files.push(path);
    }
    Ok((functions, files))
}
