//! Integration tests for `dupehound check` against scripted git repos.

use std::path::Path;
use std::process::Command;

const ORIGINAL: &str = r#"
export function formatPrice(value: number, currency: string): string {
    const rounded = Math.round(value * 100) / 100;
    const parts = rounded.toFixed(2).split(".");
    const whole = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    if (currency === "USD") {
        return "$" + whole + "." + parts[1];
    }
    if (currency === "GBP") {
        return "£" + whole + "." + parts[1];
    }
    return whole + "." + parts[1] + " " + currency;
}
"#;

const RENAMED_COPY: &str = r#"
export function displayCurrency(amount: number, code: string): string {
    const r = Math.round(amount * 100) / 100;
    const pieces = r.toFixed(2).split(".");
    const integer = pieces[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    if (code === "USD") {
        return "$" + integer + "." + pieces[1];
    }
    if (code === "GBP") {
        return "£" + integer + "." + pieces[1];
    }
    return integer + "." + pieces[1] + " " + code;
}
"#;

const UNRELATED: &str = r#"
export function median(values: number[]): number {
    const sorted = [...values].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    if (sorted.length === 0) {
        throw new Error("empty");
    }
    if (sorted.length % 2 === 1) {
        return sorted[mid];
    }
    return (sorted[mid - 1] + sorted[mid]) / 2;
}
"#;

fn sh(dir: &Path, cmd: &str, args: &[&str]) {
    let out = Command::new(cmd)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{cmd} {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_repo_with_original() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    sh(root, "git", &["init", "-q", "-b", "main"]);
    sh(root, "git", &["config", "user.email", "t@t.t"]);
    sh(root, "git", &["config", "user.name", "t"]);
    std::fs::create_dir_all(root.join("src/utils")).unwrap();
    std::fs::write(root.join("src/utils/money.ts"), ORIGINAL).unwrap();
    sh(root, "git", &["add", "-A"]);
    sh(root, "git", &["commit", "-q", "-m", "original"]);
    dir
}

fn run_check(root: &Path, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dupehound"))
        .arg("check")
        .args(extra)
        .args(["--min-tokens", "30"])
        .arg(root)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn new_renamed_duplicate_fails_with_pointer_to_original() {
    let dir = git_repo_with_original();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/checkout")).unwrap();
    std::fs::write(root.join("src/checkout/cart.ts"), RENAMED_COPY).unwrap();

    let (code, stdout, _) = run_check(root, &[]);
    assert_eq!(code, 1, "expected exit 1, output: {stdout}");
    assert!(stdout.contains("displayCurrency"));
    assert!(stdout.contains("src/utils/money.ts"));
    assert!(stdout.contains("formatPrice"));
    assert!(stdout.contains("reuse it"));
    // The concrete import that replaces the duplicate, relative to the new file.
    assert!(
        stdout.contains("import { formatPrice } from '../utils/money';"),
        "expected reuse import, output: {stdout}"
    );
}

#[test]
fn unrelated_new_code_passes() {
    let dir = git_repo_with_original();
    let root = dir.path();
    std::fs::write(root.join("src/stats.ts"), UNRELATED).unwrap();

    let (code, stdout, _) = run_check(root, &[]);
    assert_eq!(code, 0, "expected exit 0, output: {stdout}");
}

#[test]
fn moved_function_passes() {
    let dir = git_repo_with_original();
    let root = dir.path();
    // Move the function to another file: delete from origin, add elsewhere.
    std::fs::write(
        root.join("src/utils/money.ts"),
        "export const VERSION = 2;\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src/billing")).unwrap();
    std::fs::write(root.join("src/billing/format.ts"), ORIGINAL).unwrap();

    let (code, stdout, _) = run_check(root, &[]);
    assert_eq!(code, 0, "move should not fire, output: {stdout}");
}

#[test]
fn edited_function_in_place_passes() {
    let dir = git_repo_with_original();
    let root = dir.path();
    // Light edit of the original function in place.
    let edited = ORIGINAL.replace("\"GBP\"", "\"EUR\"").replace('£', "€");
    std::fs::write(root.join("src/utils/money.ts"), edited).unwrap();

    let (code, stdout, _) = run_check(root, &[]);
    assert_eq!(code, 0, "in-place edit should not fire, output: {stdout}");
}

#[test]
fn staged_duplicate_is_caught() {
    let dir = git_repo_with_original();
    let root = dir.path();
    std::fs::write(root.join("src/dup.ts"), RENAMED_COPY).unwrap();
    sh(root, "git", &["add", "-A"]);

    let (code, stdout, _) = run_check(root, &[]);
    assert_eq!(code, 1, "staged duplicate should fire, output: {stdout}");
}

#[test]
fn diff_range_mode_catches_committed_duplicate() {
    let dir = git_repo_with_original();
    let root = dir.path();
    sh(root, "git", &["checkout", "-q", "-b", "feature"]);
    std::fs::write(root.join("src/dup.ts"), RENAMED_COPY).unwrap();
    sh(root, "git", &["add", "-A"]);
    sh(root, "git", &["commit", "-q", "-m", "add duplicate"]);

    let (code, stdout, _) = run_check(root, &["--diff", "main"]);
    assert_eq!(code, 1, "PR-range duplicate should fire, output: {stdout}");
    assert!(stdout.contains("src/utils/money.ts"));
}

#[test]
fn json_output_lists_findings() {
    let dir = git_repo_with_original();
    let root = dir.path();
    std::fs::write(root.join("src/dup.ts"), RENAMED_COPY).unwrap();

    let (code, stdout, _) = run_check(root, &["--json"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["findings"].as_array().unwrap().len(), 1);
    assert_eq!(v["findings"][0]["original_name"], "formatPrice");
    assert_eq!(
        v["findings"][0]["suggestion"],
        "import { formatPrice } from './utils/money';"
    );
}
