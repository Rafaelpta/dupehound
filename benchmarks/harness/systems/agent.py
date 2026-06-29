"""Autonomous-agent system adapter (headless `claude -p`).

Gives an autonomous coding agent read-only tools and the slice directory as its
working tree, and asks it to find duplicated functions on its own -- the
realistic agent-loop framing (Ponytail-style). Cost, tokens, and the reported
pairs come from the CLI's JSON result; wall-clock is measured here.

Only needle-vs-needle pairs map to ground truth (by file basename), so the host
codebase the agent also searches does not pollute the controlled metric -- it is
the haystack that makes the task expensive at scale.

Requires the `claude` CLI on PATH and a working login/billing. If the CLI is
absent or the run errors, returns an empty prediction with an `error` note so the
orchestrator can carry on.
"""
import json
import os
import shutil
import subprocess
import time

PROMPT = (
    "You are auditing this TypeScript repository for duplicated functions: "
    "functions that are copy-paste duplicates of each other, including ones that "
    "were copied and then had ALL their identifiers (variables AND the function "
    "name) renamed -- so two duplicates may have different names but the same "
    "structure. Explore the repository with the Read, Glob, and Grep tools and "
    "find every pair of functions that duplicate each other.\n\n"
    "When done, output ONLY a JSON object on the final line, no prose, of the form:\n"
    '{"pairs": [{"file1":"<path>","func1":"<function name>",'
    '"file2":"<path>","func2":"<function name>"}, ...]}\n'
    "where each path is relative to the repository root and func1/func2 are the "
    "names of the two duplicated functions. Report a pair only when you are "
    "confident the two functions duplicate each other."
)


def _index(gt):
    return {(os.path.basename(f["file"]), f["name"]): f["id"] for f in gt["functions"]}


def _to_id(path, func, index):
    """Map a reported (file, function) to a ground-truth needle id."""
    if not isinstance(path, str) or not isinstance(func, str):
        return None
    return index.get((os.path.basename(path.strip().strip("'\"")), func.strip()))


def _extract_pairs(result_json, index):
    """Pull clone pairs from the CLI result, tolerant of shape."""
    payload = None
    if isinstance(result_json.get("structured_output"), dict):
        payload = result_json["structured_output"]
    if payload is None:
        text = result_json.get("result", "")
        start, end = text.find("{"), text.rfind("}")
        if start != -1 and end > start:
            try:
                payload = json.loads(text[start:end + 1])
            except json.JSONDecodeError:
                payload = None
    if not isinstance(payload, dict):
        return set(), 0
    predicted, reported = set(), 0
    for pair in payload.get("pairs", []):
        reported += 1
        if not isinstance(pair, dict):
            continue
        ia = _to_id(pair.get("file1"), pair.get("func1"), index)
        ib = _to_id(pair.get("file2"), pair.get("func2"), index)
        if ia and ib and ia != ib:
            predicted.add(frozenset((ia, ib)))
    return predicted, reported


def run(slice_dir, gt, model="haiku", run_idx=0, max_budget_usd=2.0, max_turns=80,
        timeout_s=1500):
    if not shutil.which("claude"):
        return _empty(model, "claude CLI not found on PATH")

    index = _index(gt)
    # Isolation is critical: without it the headless agent inherits this
    # environment's MCP servers (which include dupehound itself) and simply
    # CALLS dupehound instead of finding clones on its own -- contaminating the
    # comparison. --strict-mcp-config with no --mcp-config loads zero MCP
    # servers; --permission-mode dontAsk denies anything outside the allowlist;
    # and Task/Agent are disallowed so it cannot spawn sub-agents to escape the
    # restriction. The agent is left with genuine read-only exploration only.
    cmd = [
        "claude", "-p", PROMPT,
        "--model", model,
        "--output-format", "json",
        "--strict-mcp-config",
        "--allowedTools", "Read", "Glob", "Grep",
        "--disallowedTools", "Bash", "Write", "Edit", "Task", "Agent",
        "--permission-mode", "dontAsk",
        "--max-turns", str(max_turns),
        "--max-budget-usd", str(max_budget_usd),
    ]
    t0 = time.perf_counter()
    try:
        # hard wall-clock timeout so one hung agent can never freeze the grid;
        # stdin closed so the CLI does not wait for piped input
        proc = subprocess.run(cmd, cwd=slice_dir, capture_output=True, text=True,
                              timeout=timeout_s, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        wall_ms = (time.perf_counter() - t0) * 1000.0
        return _empty(model, "agent timed out after %ds" % timeout_s, wall_ms=wall_ms)
    wall_ms = (time.perf_counter() - t0) * 1000.0
    if not proc.stdout.strip():
        return _empty(model, "claude run failed: %s" % (proc.stderr[:200] or "no output"),
                      wall_ms=wall_ms)
    try:
        out = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return _empty(model, "could not parse claude JSON output", wall_ms=wall_ms)

    # The CLI emits a JSON result even on error (e.g. error_max_turns) with full
    # telemetry but no final answer -- capture cost/tokens and mark it DNF rather
    # than discarding the run.
    predicted, reported = _extract_pairs(out, index)
    usage = out.get("usage", {})
    tokens_in = ((usage.get("input_tokens") or 0)
                 + (usage.get("cache_creation_input_tokens") or 0)
                 + (usage.get("cache_read_input_tokens") or 0))
    result = {
        "system": "claude-agent:%s" % model,
        "kind": "agent",
        "predicted_pairs": predicted,
        "telemetry": {
            "wall_ms": wall_ms,
            "tokens_in": tokens_in,
            "tokens_out": usage.get("output_tokens"),
            "usd": out.get("total_cost_usd"),
            "num_turns": out.get("num_turns"),
            "pairs_reported": reported,
        },
        "config": {"model": model, "run_idx": run_idx,
                   "max_budget_usd": max_budget_usd, "max_turns": max_turns},
        "session_id": out.get("session_id"),
    }
    # Only a non-"success" subtype means the run did not finish (ran out of
    # turns/budget, errored mid-way). A successful run that simply mapped no
    # needles is a genuine zero, NOT a DNF -- don't discard it.
    subtype = out.get("subtype")
    if subtype and subtype != "success":
        result["error"] = "did not finish (%s)" % subtype
    return result


def _empty(model, error, wall_ms=0.0):
    return {
        "system": "claude-agent:%s" % model,
        "kind": "agent",
        "predicted_pairs": set(),
        "telemetry": {"wall_ms": wall_ms, "tokens_in": None, "tokens_out": None,
                      "usd": None, "pairs_reported": 0},
        "config": {"model": model},
        "error": error,
    }
