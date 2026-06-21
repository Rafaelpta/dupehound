//! `dupehound mcp`: expose `check` and `scan` as Model Context Protocol tools
//! over stdio, so an AI coding agent can call them itself inside its loop and
//! reuse existing code instead of rebuilding it.
//!
//! This is a hand-rolled, minimal JSON-RPC 2.0 server over newline-delimited
//! stdio. No async runtime, no network: the only thing written to stdout is a
//! protocol message, and the analysis stays the same local, deterministic, no-AI
//! engine the CLI uses. Keeping it dependency-free (just serde_json, already a
//! dependency) is why it ships in the default binary with no feature flag.

use crate::check;
use crate::cli::{CheckArgs, CommonArgs};
use crate::config::{Config, DEFAULT_SCAN_THRESHOLD};
use crate::scan;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

const SERVER_NAME: &str = "dupehound";
/// Echoed back to the client when it does not request a specific version.
const DEFAULT_PROTOCOL: &str = "2025-06-18";

/// Read JSON-RPC messages from stdin line by line, dispatch, write responses to
/// stdout. Notifications (no `id`) never get a reply. Runs until stdin closes.
pub fn run() -> Result<i32> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                write_msg(&mut out, &error(&Value::Null, -32700, "parse error"))?;
                continue;
            }
        };
        // Requests carry an `id`; notifications do not and get no response.
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let response = handle(method, msg.get("params"), &id);
        write_msg(&mut out, &response)?;
    }
    Ok(0)
}

fn write_msg(out: &mut impl Write, msg: &Value) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string(msg)?)?;
    out.flush()?;
    Ok(())
}

fn success(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn handle(method: &str, params: Option<&Value>, id: &Value) -> Value {
    match method {
        "initialize" => {
            let protocol = params
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PROTOCOL)
                .to_string();
            success(
                id,
                json!({
                    "protocolVersion": protocol,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                }),
            )
        }
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({ "tools": tool_defs() })),
        // A failing tool is a successful response whose result is flagged
        // isError, not a JSON-RPC protocol error.
        "tools/call" => match call_tool(params) {
            Ok(result) => success(id, result),
            Err(e) => success(id, tool_error(&e.to_string())),
        },
        _ => error(id, -32601, "method not found"),
    }
}

fn tool_defs() -> Value {
    json!([
        {
            "name": "check_duplication",
            "description": "Check whether code changed in a git repo duplicates code that already exists elsewhere in the repo, and point to the original function to reuse. Deterministic, local, no AI. Run it after editing so the agent reuses existing code instead of rebuilding it. Matches on structure, so it catches duplicates even when they were renamed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the git repository. Defaults to the current directory." },
                    "diff": { "type": "string", "description": "Optional git revision to compare against (merge-base, PR semantics). Without it, checks staged changes, otherwise the working tree." },
                    "threshold": { "type": "number", "description": "Similarity threshold from 0 to 1. Defaults to 0.85." }
                }
            }
        },
        {
            "name": "scan_duplication",
            "description": "Scan a whole directory for duplicate functions and return the duplication (slop) score, grade, and the duplicate clusters. Deterministic, local, no AI.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory to scan. Defaults to the current directory." },
                    "threshold": { "type": "number", "description": "Similarity threshold from 0 to 1. Defaults to 0.80." }
                }
            }
        }
    ])
}

fn call_tool(params: Option<&Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow!("missing params"))?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing tool name"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "check_duplication" => run_check(&args),
        "scan_duplication" => run_scan(&args),
        other => Err(anyhow!("unknown tool: {other}")),
    }
}

/// A clap `CommonArgs` with CLI defaults, optionally overriding the threshold.
fn common_args(threshold: Option<f64>) -> CommonArgs {
    CommonArgs {
        threshold,
        min_tokens: 40,
        excludes: Vec::new(),
        no_default_excludes: false,
        include_tests: false,
        exclude_tests: false,
        json: false,
    }
}

fn run_check(args: &Value) -> Result<Value> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let diff = args.get("diff").and_then(Value::as_str).map(str::to_string);
    let threshold = args.get("threshold").and_then(Value::as_f64);

    let check_args = CheckArgs {
        path: PathBuf::from(path),
        common: common_args(threshold),
        diff,
    };
    let outcome = check::compute(&check_args)?;

    let summary = if !outcome.had_changes {
        "No changes to check.".to_string()
    } else if outcome.findings.is_empty() {
        "No new duplicates of existing code.".to_string()
    } else {
        format!(
            "{} new duplicate(s) of existing code. Reuse the original each finding points to instead of rewriting it.",
            outcome.findings.len()
        )
    };
    let structured = json!({
        "had_changes": outcome.had_changes,
        "findings": serde_json::to_value(&outcome.findings)?,
    });
    Ok(tool_result(&summary, structured))
}

fn run_scan(args: &Value) -> Result<Value> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let threshold = args.get("threshold").and_then(Value::as_f64);

    let config = Config::from_common(&common_args(threshold), DEFAULT_SCAN_THRESHOLD);
    let output = scan::scan_path(&PathBuf::from(path), &config)?;
    let report = &output.report;
    let summary = format!(
        "Slop score {:.1}% (grade {}). {} duplicate cluster(s) across {} functions in {} files.",
        report.score.slop_percent,
        report.score.grade,
        report.clusters.len(),
        report.stats.functions,
        report.stats.files,
    );
    Ok(tool_result(&summary, serde_json::to_value(report)?))
}

/// Build an MCP tool result: a human/agent-readable text block (summary plus the
/// JSON, so any client can read it) and the structured object for clients that
/// consume `structuredContent`.
fn tool_result(summary: &str, structured: Value) -> Value {
    let pretty = serde_json::to_string_pretty(&structured).unwrap_or_default();
    json!({
        "content": [ { "type": "text", "text": format!("{summary}\n\n{pretty}") } ],
        "structuredContent": structured,
        "isError": false,
    })
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [ { "type": "text", "text": message } ],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_echoes_protocol_and_names_the_server() {
        let params = json!({ "protocolVersion": "2025-03-26" });
        let r = handle("initialize", Some(&params), &json!(1));
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "dupehound");
        assert!(r["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_falls_back_to_default_protocol() {
        let r = handle("initialize", Some(&json!({})), &json!(1));
        assert_eq!(r["result"]["protocolVersion"], DEFAULT_PROTOCOL);
    }

    #[test]
    fn tools_list_exposes_both_tools_with_object_schemas() {
        let r = handle("tools/list", None, &json!(2));
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"check_duplication"));
        assert!(names.contains(&"scan_duplication"));
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn unknown_method_is_a_protocol_error() {
        let r = handle("does/not/exist", None, &json!(3));
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn ping_returns_empty_result() {
        let r = handle("ping", None, &json!(4));
        assert!(r["result"].is_object());
        assert!(r.get("error").is_none());
    }

    #[test]
    fn unknown_tool_is_an_is_error_result_not_a_protocol_error() {
        let params = json!({ "name": "nope", "arguments": {} });
        let r = handle("tools/call", Some(&params), &json!(5));
        // Protocol-level success, tool-level error.
        assert!(r.get("error").is_none());
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn missing_tool_name_is_an_is_error_result() {
        let params = json!({ "arguments": {} });
        let r = handle("tools/call", Some(&params), &json!(6));
        assert_eq!(r["result"]["isError"], true);
    }
}
