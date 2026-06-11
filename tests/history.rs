//! Integration test for `dupehound history` on a scripted repo with a
//! deliberate duplication ramp at controlled commit dates.

use std::path::Path;
use std::process::Command;

fn sh_env(dir: &Path, date: &str, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn base_fn(name: &str, salt: u32) -> String {
    format!(
        r#"
export function {name}(values: number[], limit: number): number {{
    let total = 0;
    let count = 0;
    for (const value of values) {{
        if (value > limit) {{
            total += value * {salt};
            count += 1;
        }} else if (value < -limit) {{
            total -= value * {salt};
        }}
    }}
    if (count === 0) {{
        return 0;
    }}
    return Math.round((total / count) * 100) / 100;
}}
"#
    )
}

#[test]
fn history_charts_a_duplication_ramp() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    sh_env(root, "2024-01-15T12:00:00", &["init", "-q", "-b", "main"]);
    sh_env(
        root,
        "2024-01-15T12:00:00",
        &["config", "user.email", "t@t.t"],
    );
    sh_env(root, "2024-01-15T12:00:00", &["config", "user.name", "t"]);

    // Months 1-6: clean growth, distinct functions.
    for (i, month) in ["01", "02", "03", "04", "05", "06"].iter().enumerate() {
        let path = root.join(format!("module_{i}.ts"));
        std::fs::write(&path, base_fn(&format!("uniqueLogic{i}"), i as u32 + 3)).unwrap();
        let date = format!("2024-{month}-15T12:00:00");
        sh_env(root, &date, &["add", "-A"]);
        sh_env(
            root,
            &date,
            &["commit", "-q", "-m", &format!("month {month}")],
        );
    }
    // Months 7-12: the "AI adoption" ramp — renamed copies of module_0 pile up.
    for (i, month) in ["07", "08", "09", "10", "11", "12"].iter().enumerate() {
        let path = root.join(format!("copy_{i}.ts"));
        std::fs::write(&path, base_fn(&format!("rewrittenHelper{i}"), 3)).unwrap();
        let date = format!("2024-{month}-15T12:00:00");
        sh_env(root, &date, &["add", "-A"]);
        sh_env(
            root,
            &date,
            &["commit", "-q", "-m", &format!("month {month}")],
        );
    }

    let out = Command::new(env!("CARGO_BIN_EXE_dupehound"))
        .args(["history", "--json", "--no-card", "--min-tokens", "30"])
        .arg(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "history failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let series = report["series"].as_array().unwrap();
    assert!(
        series.len() >= 10,
        "expected ~12 snapshots, got {}",
        series.len()
    );

    // Slop must be 0 early and high at the end.
    let first = series[0]["slop_percent"].as_f64().unwrap();
    let last = series.last().unwrap()["slop_percent"].as_f64().unwrap();
    assert_eq!(first, 0.0, "first snapshot should be clean");
    assert!(last > 20.0, "final snapshot should be sloppy, got {last}");

    // The ramp should be flagged with a growth callout pointing at the
    // clean era (some month in 2024 before the ramp completes).
    let since = report["growth_since"].as_str();
    assert!(
        since.is_some_and(|s| s.starts_with("2024-0")),
        "expected growth_since in the clean era, got {since:?}"
    );

    // Monotonic-ish: the back half is never cleaner than the front half.
    let mid = series[series.len() / 2]["slop_percent"].as_f64().unwrap();
    assert!(last >= mid && mid >= first);
}

#[test]
fn history_on_non_repo_fails_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dupehound"))
        .args(["history", "--no-card"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("git"), "unhelpful error: {err}");
}
