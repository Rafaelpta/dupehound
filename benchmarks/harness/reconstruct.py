#!/usr/bin/env python3
"""Reconstruct the scaling report from the agent transcripts already produced.

The live orchestrator only writes its result file at the very end; when a run is
killed (e.g. after a hung agent), we still want the honest partial picture. This
reads each completed agent session from the Claude transcripts, scores what it
actually reported against the realistic ground truth, re-runs dupehound fresh
(free) for the baseline, and writes the report -- marking any (model, slice)
cell with no completed run as DNF (did not finish), never as "found zero".
"""
import glob
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import paths            # noqa: E402
import score            # noqa: E402
import run_scaling      # noqa: E402
from systems import agent as agent_sys      # noqa: E402
from systems import dupehound as dh_sys     # noqa: E402

PROJECTS = os.path.expanduser("~/.claude/projects")
WINDOW_S = 16 * 3600  # only this run's transcripts


def short_model(model_id):
    for k in ("haiku", "sonnet", "opus"):
        if k in (model_id or ""):
            return k
    return None


def parse_transcript(path):
    model = None
    tin = tout = cost = turns = 0
    final = None
    ts = []
    for line in open(path):
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
        u = m.get("usage") or {}
        if u:
            turns += 1
            tin += (u.get("input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0) \
                + (u.get("cache_read_input_tokens") or 0)
            tout += u.get("output_tokens") or 0
        mu = m.get("modelUsage") or {}
        for v in mu.values():
            if isinstance(v, dict):
                cost += v.get("costUSD") or 0
        c = m.get("content")
        if isinstance(c, list):
            for b in c:
                if isinstance(b, dict) and b.get("type") == "text" and '"pairs"' in str(b.get("text", "")):
                    final = b["text"]
    wall_ms = None
    if len(ts) >= 2:
        try:
            import datetime
            t0 = datetime.datetime.fromisoformat(ts[0].replace("Z", "+00:00"))
            t1 = datetime.datetime.fromisoformat(ts[-1].replace("Z", "+00:00"))
            wall_ms = (t1 - t0).total_seconds() * 1000.0
        except (ValueError, TypeError):
            wall_ms = None
    return {"model": short_model(model), "final": final, "tokens_in": tin,
            "tokens_out": tout, "usd": cost or None, "turns": turns, "wall_ms": wall_ms}


def slice_project_dir(slice_dir):
    # Claude Code keys its project dirs by the realpath with /, _ and . -> -
    key = re.sub(r"[/_.]", "-", os.path.realpath(slice_dir))
    return os.path.join(PROJECTS, key)


def main():
    base = os.path.join(paths.data_dir(), "slices")
    meta = json.load(open(os.path.join(base, "slices.json")))
    slices = sorted(meta["slices"], key=lambda s: s["host_loc"])
    now = time.time()
    models = ["haiku", "sonnet", "opus"]

    dh_rows, agent_rows, notes = [], [], []
    dh_version = None

    for s in slices:
        gt = json.load(open(s["gt_path"]))
        index = agent_sys._index(gt)

        # dupehound baseline (fresh, free)
        dh_runs = [dh_sys.run(s["dir"], gt, 0.80, 20) for _ in range(3)]
        dh_version = dh_runs[0]["system"]
        dh_rows.append(run_scaling.attach_size(score.aggregate(dh_runs, gt), s))

        # agent runs from transcripts
        pdir = slice_project_dir(s["dir"])
        by_model = {m: [] for m in models}
        for f in glob.glob(os.path.join(pdir, "*.jsonl")):
            if now - os.path.getmtime(f) > WINDOW_S:
                continue
            p = parse_transcript(f)
            if not p["model"] or p["model"] not in by_model:
                continue
            if not p["final"]:
                continue  # incomplete session -> not counted
            pred, _ = agent_sys._extract_pairs({"result": p["final"]}, index)
            by_model[p["model"]].append({
                "system": "claude-agent:%s" % p["model"], "kind": "agent",
                "simulated": False, "predicted_pairs": pred,
                "telemetry": {"wall_ms": p["wall_ms"], "tokens_in": p["tokens_in"],
                              "tokens_out": p["tokens_out"], "usd": p["usd"]},
            })
        for m in models:
            runs = by_model[m]
            if runs:
                agent_rows.append(run_scaling.attach_size(score.aggregate(runs, gt), s))
            else:
                notes.append("%s on %s: all runs hung/incomplete (DNF)" % (m, s["name"]))
                agent_rows.append(run_scaling.attach_size(_dnf_row("claude-agent:%s" % m, gt), s))

    rmeta = {
        "date": "2026-06-28", "tag": "ts-realistic", "repo": meta["repo"],
        "commit": meta["commit"], "seed": meta["seed"], "profile": meta.get("lang") and "realistic",
        "clone_type_counts": json.load(open(slices[0]["gt_path"])).get("clone_type_counts", {}),
        "slices": slices, "needle_functions": slices[0]["needle_functions"],
        "needle_pairs": slices[0]["needle_pairs"], "dupehound": dh_version,
        "models": models, "dh_runs": 3, "agent_runs": "2 (partial; reconstructed)",
        "spent": 0.0, "cap": 0.0, "notes": notes,
        "reconstructed": True,
    }
    out = os.path.join(HERE, "..", "results", "2026-06-28-ts-realistic")
    with open(os.path.abspath(out) + ".json", "w") as f:
        json.dump({"meta": run_scaling._meta_json(rmeta),
                   "dupehound": run_scaling._rows_json(dh_rows),
                   "agents": run_scaling._rows_json(agent_rows)}, f, indent=2)
    run_scaling.write_markdown(os.path.abspath(out) + ".md", rmeta, dh_rows, agent_rows)
    print("wrote", os.path.abspath(out) + ".md")
    for r in dh_rows + agent_rows:
        print("  %-22s %-9s tp=%.1f" % (r["system"], "{:,}".format(r["host_loc"]), r["tp_mean"]))


def _dnf_row(system, gt):
    return {
        "system": system, "kind": "agent", "simulated": False, "runs": 0,
        "f1_mean": None, "f1_std": 0.0, "recall_mean": 0.0, "recall_std": 0.0,
        "precision_mean": 0.0, "precision_std": 0.0, "mcc_mean": 0.0,
        "recall_by_type": {t: 0.0 for t in ("1", "2", "3", "4")},
        "tp_mean": 0.0, "fp_mean": 0.0, "fn_mean": float(len(gt["pairs"])),
        "wall_ms_mean": None, "wall_ms_std": 0.0,
        "tokens_in_mean": None, "tokens_out_mean": None,
        "usd_mean": None, "cost_per_tp": None, "determinism": None,
    }


if __name__ == "__main__":
    main()
