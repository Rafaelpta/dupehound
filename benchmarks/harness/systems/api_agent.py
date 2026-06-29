"""OpenAI (and other OpenAI-compatible) autonomous-agent adapter.

Gives a chat model the SAME three read-only tools as the Claude arm -- glob,
grep, read_file -- all sandboxed to the slice directory, and asks it to find
duplicated functions on its own. Returns the same {predicted_pairs, telemetry}
contract as systems/agent.py, scored via the shared (file, function) index.

Isolation is by construction: the only tools are read-only file ops confined to
the slice dir, so the model cannot run dupehound, spawn sub-agents, or read the
ground truth (which lives outside the scanned dir). The OPENAI_API_KEY is read
from the environment only.
"""
import glob as globmod
import json
import os
import subprocess
import time

from systems.agent import PROMPT, _index, _extract_pairs

# USD per 1M tokens (input, output). Best-effort; edit to the provider's current
# published prices for citable cost. Token counts are always exact (from usage).
PRICES = {
    "gpt-5.5":       (1.25, 10.0),
    "gpt-5.4-mini":  (0.25, 2.0),
    "gpt-5-mini":    (0.25, 2.0),
}

MAX_READ_CHARS = 30000
MAX_GREP_LINES = 120
MAX_GLOB = 200

TOOLS = [
    {"type": "function", "function": {
        "name": "glob", "description": "List files matching a glob pattern (e.g. '**/*.ts').",
        "parameters": {"type": "object", "properties": {
            "pattern": {"type": "string"}}, "required": ["pattern"]}}},
    {"type": "function", "function": {
        "name": "grep", "description": "Search file contents with a regex; returns file:line matches.",
        "parameters": {"type": "object", "properties": {
            "pattern": {"type": "string"},
            "glob": {"type": "string", "description": "optional file glob filter, e.g. '*.ts'"}},
            "required": ["pattern"]}}},
    {"type": "function", "function": {
        "name": "read_file", "description": "Read a UTF-8 text file (truncated if large).",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string"}}, "required": ["path"]}}},
]


def _safe(root, path):
    full = os.path.realpath(os.path.join(root, path))
    return full if full.startswith(os.path.realpath(root) + os.sep) else None


def _tool_glob(root, pattern):
    hits = globmod.glob(os.path.join(root, pattern), recursive=True)
    rels = sorted(os.path.relpath(h, root) for h in hits if os.path.isfile(h))
    extra = len(rels) - MAX_GLOB
    out = "\n".join(rels[:MAX_GLOB])
    return out + ("\n... (%d more)" % extra if extra > 0 else "") or "(no matches)"


def _tool_grep(root, pattern, gl=None):
    cmd = ["grep", "-rIn", "--include", gl or "*.ts", "-e", pattern, root]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    except (subprocess.TimeoutExpired, OSError):
        return "(grep error/timeout)"
    lines = [ln.replace(root + os.sep, "") for ln in p.stdout.splitlines()]
    extra = len(lines) - MAX_GREP_LINES
    out = "\n".join(lines[:MAX_GREP_LINES])
    return out + ("\n... (%d more matches)" % extra if extra > 0 else "") or "(no matches)"


def _tool_read(root, path):
    full = _safe(root, path)
    if not full or not os.path.isfile(full):
        return "(no such file)"
    try:
        with open(full, encoding="utf-8") as f:
            data = f.read(MAX_READ_CHARS + 1)
    except (UnicodeDecodeError, OSError):
        return "(unreadable)"
    return data[:MAX_READ_CHARS] + ("\n... (truncated)" if len(data) > MAX_READ_CHARS else "")


def _dispatch(root, name, args):
    if name == "glob":
        return _tool_glob(root, args.get("pattern", ""))
    if name == "grep":
        return _tool_grep(root, args.get("pattern", ""), args.get("glob"))
    if name == "read_file":
        return _tool_read(root, args.get("path", ""))
    return "(unknown tool)"


def run(slice_dir, gt, model="gpt-5.5", run_idx=0, max_iters=40, timeout_s=900,
        base_url=None):
    try:
        from openai import OpenAI
    except ImportError:
        return _empty(model, "openai SDK not installed")
    if not os.environ.get("OPENAI_API_KEY"):
        return _empty(model, "OPENAI_API_KEY not set")

    client = OpenAI(base_url=base_url) if base_url else OpenAI()
    index = _index(gt)
    messages = [
        {"role": "system", "content": PROMPT},
        {"role": "user", "content": "The repository root is the current directory. "
         "Use the glob, grep and read_file tools to explore it and find duplicated "
         "functions. Paths are relative to the root."},
    ]
    tin = tout = 0
    final_text = ""
    t0 = time.perf_counter()
    err = None
    for _ in range(max_iters):
        if time.perf_counter() - t0 > timeout_s:
            err = "timed out after %ds" % timeout_s
            break
        try:
            resp = client.chat.completions.create(
                model=model, messages=messages, tools=TOOLS, tool_choice="auto")
        except Exception as e:  # noqa: BLE001
            err = "api error: %s" % repr(e)[:200]
            break
        u = getattr(resp, "usage", None)
        if u:
            tin += u.prompt_tokens or 0
            tout += u.completion_tokens or 0
        msg = resp.choices[0].message
        if not msg.tool_calls:
            final_text = msg.content or ""
            break
        messages.append({"role": "assistant", "content": msg.content,
                         "tool_calls": [tc.model_dump() for tc in msg.tool_calls]})
        for tc in msg.tool_calls:
            try:
                args = json.loads(tc.function.arguments or "{}")
            except json.JSONDecodeError:
                args = {}
            result = _dispatch(slice_dir, tc.function.name, args)
            messages.append({"role": "tool", "tool_call_id": tc.id, "content": result})
    wall_ms = (time.perf_counter() - t0) * 1000.0
    if not final_text and not err:
        err = "did not finish (max_iters)"

    predicted, reported = _extract_pairs({"result": final_text}, index)
    price = PRICES.get(model)
    usd = (tin / 1e6 * price[0] + tout / 1e6 * price[1]) if price else None
    out = {
        "system": "openai-agent:%s" % model, "kind": "agent",
        "predicted_pairs": predicted,
        "telemetry": {"wall_ms": wall_ms, "tokens_in": tin, "tokens_out": tout,
                      "usd": usd, "pairs_reported": reported},
        "config": {"model": model, "run_idx": run_idx, "max_iters": max_iters},
    }
    if err and not predicted:
        out["error"] = err
    return out


def _empty(model, error, wall_ms=0.0):
    return {"system": "openai-agent:%s" % model, "kind": "agent",
            "predicted_pairs": set(),
            "telemetry": {"wall_ms": wall_ms, "tokens_in": None, "tokens_out": None,
                          "usd": None, "pairs_reported": 0},
            "config": {"model": model}, "error": error}
