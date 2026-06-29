#!/usr/bin/env python3
"""Reconstruct granular PER-RUN agent telemetry from the clean-run transcripts.

The orchestrator only stores aggregates; for the paper we need each individual
run's tokens, wall time, cost, turns, recovered count and status. This parses the
most recent N agent transcripts per slice (N = models x runs of the clean run),
scores each against the slice ground truth, and prints a per-run table plus
per-cell means (to cross-check against the report) and cost projections.

Read-only. Usage: python3 granular.py
"""
import datetime
import glob
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import paths            # noqa: E402
from systems import agent as A  # noqa: E402

PROJECTS = os.path.expanduser("~/.claude/projects")
AGENT_SLICES = {"microsoft_vscode-10k", "microsoft_vscode-100k", "microsoft_vscode-1m"}
RUNS_PER_SLICE = 6  # 3 models x 2 runs


def project_dir(slice_dir):
    return os.path.join(PROJECTS, re.sub(r"[/_.]", "-", os.path.realpath(slice_dir)))


def short(model):
    for k in ("haiku", "sonnet", "opus"):
        if k and k in (model or ""):
            return k
    return None


def parse(f):
    model = None
    tin = tout = cost = turns = 0
    ts = []
    final = None
    for line in open(f):
        try:
            e = json.loads(line)
        except json.JSONDecodeError:
            continue
        if e.get("timestamp"):
            ts.append(e["timestamp"])
        m = e.get("message", {})
        if not isinstance(m, dict):
            continue
        if m.get("model"):
            model = m["model"]
        mu = m.get("modelUsage") or {}
        for k in mu:
            if short(k):
                model = model or k
        for v in mu.values():
            if isinstance(v, dict):
                cost += v.get("costUSD") or 0
        u = m.get("usage") or {}
        if u:
            turns += 1
            tin += (u.get("input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0) \
                + (u.get("cache_read_input_tokens") or 0)
            tout += u.get("output_tokens") or 0
        c = m.get("content")
        if isinstance(c, list):
            for b in c:
                if isinstance(b, dict) and b.get("type") == "text" and '"pairs"' in str(b.get("text", "")):
                    final = b["text"]
    wall = None
    if len(ts) >= 2:
        try:
            t0 = datetime.datetime.fromisoformat(ts[0].replace("Z", "+00:00"))
            t1 = datetime.datetime.fromisoformat(ts[-1].replace("Z", "+00:00"))
            wall = (t1 - t0).total_seconds()
        except (ValueError, TypeError):
            wall = None
    return {"model": short(model), "tin": tin, "tout": tout, "cost": cost,
            "turns": turns, "wall": wall, "final": final}


def load_report_costs():
    """(model, host_loc) -> usd_mean from the clean-run report (real -p cost)."""
    rp = os.path.join(HERE, "..", "results", "2026-06-28-ts-final-claude.json")
    out = {}
    try:
        d = json.load(open(rp))
    except (OSError, json.JSONDecodeError):
        return out
    for r in d.get("agents", []):
        model = r["system"].split(":")[-1]
        out[(model, r["host_loc"])] = r.get("usd_mean")
    return out


def latest_per_model(pdir):
    """2 most recent transcripts per model = the clean run's 2 runs/model.

    Transcripts with wall > 1100s are dropped: the clean run capped every run at
    900s, so a longer one can only be from an older pre-timeout run leaking in.
    """
    parsed = []
    for f in glob.glob(pdir + "/*.jsonl"):
        p = parse(f)
        if p["model"] and not (p["wall"] and p["wall"] > 1100):
            p["_mtime"] = os.path.getmtime(f)
            parsed.append(p)
    by_model = {}
    for p in sorted(parsed, key=lambda x: x["_mtime"]):
        by_model.setdefault(p["model"], []).append(p)
    out = []
    for m, ps in by_model.items():
        out += ps[-2:]
    return out


def main():
    base = os.path.join(paths.data_dir(), "slices")
    meta = json.load(open(os.path.join(base, "slices.json")))
    slmap = {s["name"]: s for s in meta["slices"]}
    report_cost = load_report_costs()

    print("# Granular per-run agent telemetry (clean run, n=2)")
    print("# tokens & time per run are exact (from transcripts); $ is per-cell (from the -p cost accounting)\n")
    print("%-6s %-7s %-3s %10s %8s %6s %8s  %s" %
          ("slice", "model", "run", "tok_in", "tok_out", "turns", "wall_s", "found/status"))
    rows = []
    for name in sorted(AGENT_SLICES, key=lambda n: slmap[n]["host_loc"]):
        s = slmap[name]
        gt = json.load(open(s["gt_path"]))
        index = A._index(gt)
        total = len(gt["pairs"])
        pdir = project_dir(s["dir"])
        runs = sorted(latest_per_model(pdir), key=lambda x: (x["model"], x["_mtime"]))
        per_model = {}
        for p in runs:
            if p["final"]:
                pred, _ = A._extract_pairs({"result": p["final"]}, index)
                found = len({fr for fr in pred})
                status = "%d/%d" % (found, total)
            else:
                found = 0
                status = "DNF"
            ri = per_model.get(p["model"], 0)
            per_model[p["model"]] = ri + 1
            row = {"slice": name.split("-")[-1], "loc": s["host_loc"], "model": p["model"],
                   "run": ri, "tin": p["tin"], "tout": p["tout"], "cost": p["cost"],
                   "turns": p["turns"], "wall": p["wall"], "found": found, "status": status}
            rows.append(row)
            print("%-6s %-7s %-3d %10s %8s %6d %8s  %s" % (
                row["slice"], row["model"], row["run"], f"{p['tin']:,}", f"{p['tout']:,}",
                p["turns"], ("%.0f" % p["wall"]) if p["wall"] else "?", status))

    # per-cell means (cross-check vs report) + cost projections
    print("\n# Per-cell means (cross-check vs report)")
    cells = {}
    for r in rows:
        cells.setdefault((r["loc"], r["model"]), []).append(r)
    total_cost = 0.0
    for (loc, model), rs in sorted(cells.items()):
        comp = [r for r in rs if r["status"] != "DNF"]
        f_mean = sum(r["found"] for r in comp) / len(comp) if comp else 0
        c_mean = report_cost.get((model, loc)) or 0.0
        total_cost += c_mean * len(rs)
        ti_mean = sum(r["tin"] for r in rs) / len(rs)
        w = [r["wall"] for r in rs if r["wall"]]
        w_mean = sum(w) / len(w) if w else 0
        print("  %9s %-7s found_mean=%4.1f cost/run=$%.2f tok_in_mean=%12s wall_mean=%4.0fs (n=%d, dnf=%d)" % (
            f"{loc:,}", model, f_mean, c_mean, f"{ti_mean:,.0f}", w_mean, len(rs), len(rs) - len(comp)))

    total_tin = sum(r["tin"] for r in rows)
    print("\n# Totals: %d runs, ~$%.2f notional cost (Max subscription = $0 real), %s input tokens"
          % (len(rows), total_cost, f"{total_tin:,}"))
    # projection from the 1M cells: blended $/Mtok and cost to scan the full corpus
    big_locs = [(loc, model) for (loc, model) in cells if loc >= 1_000_000]
    big_tok = sum(r["tin"] for r in rows if r["loc"] >= 1_000_000)
    big_cost = sum((report_cost.get((m, loc)) or 0.0) * len([r for r in rows if r["loc"] == loc and r["model"] == m])
                   for (loc, m) in big_locs)
    n_big = len([r for r in rows if r["loc"] >= 1_000_000])
    if n_big:
        tpl = big_tok / n_big / 1_000_000 * 1000     # input tok per 1k LOC
        cpl = big_cost / n_big / 1_000_000 * 1000    # $ per 1k LOC (notional)
        full = meta["available_loc"]
        print("# Projection @1M: ~%s input tok / 1k LOC, ~$%.4f / 1k LOC (notional)" % (f"{tpl:,.0f}", cpl))
        print("# One agent pass over the full %s-LOC corpus: ~$%.2f notional (dupehound: $0, <2s)"
              % (f"{full:,}", cpl * full / 1000))


if __name__ == "__main__":
    main()
