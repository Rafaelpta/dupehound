#!/usr/bin/env python3
"""Scaling-curve orchestrator: dupehound vs autonomous agents over repo size.

Plants the same needles into nested slices of a large real TypeScript repo and
runs every system on every slice. dupehound runs on all slices for free; the
agent tier is gated by a hard budget cap with an up-front projection from a
cheap probe on the smallest slice.

Writes results/<date>-ts-scaling.{md,json} with a per-size scoreboard so the
divergence is visible: dupehound flat in time/cost, agents climbing.

Examples:
    # dupehound only (free):
    python3 run_scaling.py --no-agent --tag dupehound-scaling

    # full run with the Claude agent tiers, $50 cap:
    python3 run_scaling.py --models haiku sonnet opus --max-usd 50 --tag ts-scaling
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import budget  # noqa: E402
import host    # noqa: E402
import paths   # noqa: E402
import score   # noqa: E402
from systems import dupehound as dh_sys  # noqa: E402
from systems import agent as agent_sys   # noqa: E402
from systems import api_agent            # noqa: E402

from run import time_fmt, usd_fmt, fmt, same_answer  # noqa: E402


def tok_fmt(n):
    if n is None:
        return "n/a"
    n = int(round(n))
    if n >= 1000:
        return "%dk" % round(n / 1000)
    return str(n)


def load_gt(s):
    """Load a slice's ground truth from its out-of-corpus location."""
    with open(s["gt_path"]) as f:
        return json.load(f)


def attach_size(agg, s):
    agg["slice"] = s["name"]
    agg["host_loc"] = s["host_loc"]
    agg["host_files"] = s["host_files"]
    return agg


def run_dupehound(slices, runs, threshold, min_tokens):
    rows = []
    version = None
    for s in slices:
        gt = load_gt(s)
        rs = [dh_sys.run(s["dir"], gt, threshold, min_tokens) for _ in range(runs)]
        version = rs[0]["system"]
        rows.append(attach_size(score.aggregate(rs, gt), s))
    return version, rows


def _is_openai(model):
    return model.startswith(("gpt", "o1", "o3", "o4", "chatgpt"))


def _run_one(s, model, ri, per_run_cap, max_turns, timeout_s):
    gt = load_gt(s)
    if _is_openai(model):
        r = api_agent.run(s["dir"], gt, model=model, run_idx=ri,
                          max_iters=max_turns, timeout_s=timeout_s)
    else:
        r = agent_sys.run(s["dir"], gt, model=model, run_idx=ri,
                         max_budget_usd=per_run_cap, max_turns=max_turns, timeout_s=timeout_s)
    return s, model, ri, r


def _dnf_row(system, host_loc, n_pairs):
    return {"system": system, "kind": "agent", "simulated": False, "runs": 0,
            "f1_mean": None, "recall_mean": 0.0, "precision_mean": 0.0, "mcc_mean": 0.0,
            "f1_std": 0.0, "recall_std": 0.0, "precision_std": 0.0,
            "recall_by_type": {t: 0.0 for t in ("1", "2", "3", "4")},
            "tp_mean": 0.0, "fp_mean": 0.0, "fn_mean": float(n_pairs),
            "wall_ms_mean": None, "wall_ms_std": 0.0,
            "tokens_in_mean": None, "tokens_out_mean": None,
            "usd_mean": None, "cost_per_tp": None, "determinism": None,
            "host_loc": host_loc}


def run_agents(slices, models, agent_runs, per_run_cap, max_turns, timeout_s, concurrency):
    """Run the agent grid with bounded concurrency. Every run is independently
    capped (Claude --max-budget-usd + timeout; OpenAI max_iters + timeout), so no
    run can hang the grid. Cells whose every run errors are reported DNF."""
    from concurrent.futures import ThreadPoolExecutor, as_completed
    tasks = [(s, model, ri) for s in slices for model in models for ri in range(agent_runs)]
    runs_by_cell = {(s["name"], model): [] for s in slices for model in models}
    notes, spent = [], 0.0
    print("running %d agent runs, %d at a time..." % (len(tasks), concurrency))
    with ThreadPoolExecutor(max_workers=concurrency) as ex:
        futs = [ex.submit(_run_one, s, m, ri, per_run_cap, max_turns, timeout_s)
                for (s, m, ri) in tasks]
        for f in as_completed(futs):
            s, model, ri, r = f.result()
            spent += r["telemetry"].get("usd") or 0.0
            if r.get("error"):
                notes.append("%s on %s run %d: %s" % (model, s["name"], ri, r["error"]))
            runs_by_cell[(s["name"], model)].append(r)
            print("  done: %-22s %-9s run %d%s" % (
                r["system"], s["name"].split("-")[-1], ri,
                "  ERROR" if r.get("error") else ""))

    rows = []
    for s in slices:
        for model in models:
            cr = runs_by_cell[(s["name"], model)]
            good = [r for r in cr if not r.get("error")]
            sysname = cr[0]["system"] if cr else ("agent:%s" % model)
            if good:
                rows.append(attach_size(score.aggregate(good, load_gt(s)), s))
            else:
                notes.append("%s on %s: all runs failed (DNF)" % (model, s["name"]))
                rows.append(_dnf_row(sysname, s["host_loc"], s["needle_pairs"]))
    return rows, spent, notes


def write_markdown(path, meta, dh_rows, agent_rows):
    by_system = {}
    for r in dh_rows + agent_rows:
        by_system.setdefault(r["system"], []).append(r)
    for rows in by_system.values():
        rows.sort(key=lambda r: r["host_loc"])

    total_pairs = meta["needle_pairs"]
    L = []
    L.append("# Scaling benchmark: finding planted duplicates in a large repo -- %s" % meta["date"])
    L.append("")
    L.append("> Same %d planted duplicate pairs, hidden in growing slices of a real "
             "TypeScript codebase (%s @ `%s`). Who still finds them as the haystack "
             "grows, and what does it cost? Method: [METHODOLOGY.md](../METHODOLOGY.md)."
             % (total_pairs, meta["repo"], meta["commit"][:10]))
    L.append("")
    L.append("## Provenance")
    L.append("")
    L.append("| field | value |")
    L.append("|---|---|")
    L.append("| host repo | %s @ `%s` |" % (meta["repo"], meta["commit"][:10]))
    L.append("| slices (host LOC) | %s |" % ", ".join("%d" % s["host_loc"] for s in meta["slices"]))
    tc = meta.get("clone_type_counts", {})
    mix = " ".join("T%s:%s" % (t, c) for t, c in sorted(tc.items()))
    L.append("| duplication profile | %s (%s) |" % (meta.get("profile", "?"), mix))
    L.append("| planted needles | %d functions, %d clone pairs (seed %d) |"
             % (meta["needle_functions"], total_pairs, meta["seed"]))
    L.append("| dupehound | `%s` |" % meta["dupehound"])
    L.append("| agent models | %s |" % (", ".join(meta["models"]) or "none"))
    L.append("| dupehound runs / agent runs | %s / %s |" % (meta["dh_runs"], meta["agent_runs"]))
    L.append("| agent token cost | ~$%.2f (OpenAI real $; Claude on Max subscription) |" % meta.get("spent", 0.0))
    L.append("| harness | Claude via `claude -p` (its own agent loop); OpenAI via our Python tool-loop (Glob/Grep/Read) |")
    L.append("")

    L.append("## Scoreboard by repo size")
    L.append("")
    L.append("Each cell: duplicates found of %d, with time and cost per run." % total_pairs)
    L.append("")
    # Map agent runs that hung/timed-out/were-skipped from the notes, so the
    # scoreboard can mark a cell "did not finish" instead of pretending it found
    # zero. A cell is DNF only when EVERY one of its runs failed.
    bad = {}  # (model, slice_name) -> count of failed/hung/skipped runs
    slice_loc = {s["name"]: s["host_loc"] for s in meta["slices"]}
    for note in meta.get("notes", []):
        m = re.match(r"(?:skipped )?([\w.-]+) on ([\w./-]+)", note)
        if m:
            bad[(m.group(1), m.group(2))] = bad.get((m.group(1), m.group(2)), 0) + 1
    loc_to_slice = {v: k for k, v in slice_loc.items()}

    def found_cell(r):
        if r.get("runs") == 0:  # every run failed -> genuinely did not finish
            return "DNF (hung)"
        n = round(r["tp_mean"])
        model = r["system"].split(":")[-1]
        nbad = bad.get((model, loc_to_slice.get(r["host_loc"], "")), 0)
        if nbad:
            return "%d of %d  (%d run hung)" % (n, total_pairs, nbad)
        return "%d of %d" % (n, total_pairs)

    L.append("| system | repo size (LOC) | duplicates found | false alarms | time / run | tokens in/out | cost / run |")
    L.append("|---|---|---|---|---|---|---|")
    for system in sorted(by_system):
        for r in by_system[system]:
            L.append("| %s | %s | %s | %s | %s | %s | %s |" % (
                system, "{:,}".format(r["host_loc"]),
                found_cell(r),
                "%d" % round(r["fp_mean"]),
                time_fmt(r["wall_ms_mean"]),
                "%s / %s" % (tok_fmt(r.get("tokens_in_mean")), tok_fmt(r.get("tokens_out_mean"))),
                usd_fmt(r["usd_mean"])))
    L.append("")
    L.append("*DNF / \"run hung\" = an agent run timed out or was killed without "
             "finishing; its recall is not a real \"found zero\", just an "
             "unfinished exploration. dupehound never hangs.*")
    L.append("")

    L.append("## What it costs to hold recall as the repo grows")
    L.append("")
    sizes = sorted({r["host_loc"] for r in dh_rows + agent_rows})
    hdr = " | ".join("{:,}".format(s) for s in sizes)
    L.append("| system | " + hdr + " |")
    L.append("|---|" + "|".join("---" for _ in sizes) + "|")
    for system in sorted(by_system):
        cells = {r["host_loc"]: r for r in by_system[system]}
        row = []
        for s in sizes:
            r = cells.get(s)
            row.append("%s / %s" % (time_fmt(r["wall_ms_mean"]), usd_fmt(r["usd_mean"])) if r else "-")
        L.append("| %s | %s |" % (system, " | ".join(row)))
    L.append("")

    L.append("## Recall by clone type, per model and repo size")
    L.append("")
    tc = meta.get("clone_type_counts", {})
    L.append("How many of each kind each system recovered. T1 = exact copy-paste, "
             "T2 = renamed copy-paste (the bulk of real duplication), T3 = renamed + "
             "a few edited lines, T4 = same behaviour rewritten differently. "
             "Totals planted: %s." % " ".join("T%s=%s" % (t, c) for t, c in sorted(tc.items())))
    L.append("")
    types = ["1", "2", "3", "4"]
    labels = {"1": "copy-paste (T1)", "2": "renamed (T2)", "3": "edited (T3)", "4": "rewritten (T4)"}
    L.append("| system | repo size | " + " | ".join(labels[t] for t in types) + " |")
    L.append("|---|---|" + "|".join("---" for _ in types) + "|")

    def type_cell(r, t):
        total = tc.get(t, 0)
        if not total:
            return "-"
        rec = r["recall_by_type"].get(t)
        if rec is None or (r.get("runs") == 0):
            return "DNF" if r.get("runs") == 0 else "-"
        return "%d/%d" % (round(rec * total), total)

    for system in sorted(by_system):
        for r in by_system[system]:
            L.append("| %s | %s | %s |" % (
                system, "{:,}".format(r["host_loc"]),
                " | ".join(type_cell(r, t) for t in types)))
    L.append("")

    if meta.get("notes"):
        L.append("## Notes")
        L.append("")
        for n in meta["notes"]:
            L.append("- %s" % n)
        L.append("")
    L.append("---")
    L.append("Regenerate: `python3 harness/run_scaling.py --tag %s`" % meta["tag"])
    L.append("")
    with open(path, "w") as f:
        f.write("\n".join(L))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="microsoft/vscode")
    ap.add_argument("--commit", default=None)
    ap.add_argument("--targets", type=int, nargs="+",
                    default=[10000, 100000, 500000, 1000000])
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--profile", default="realistic")
    ap.add_argument("--threshold", type=float, default=0.80)
    ap.add_argument("--min-tokens", type=int, default=20)
    ap.add_argument("--models", nargs="*", default=["haiku", "sonnet", "opus"])
    ap.add_argument("--no-agent", action="store_true")
    ap.add_argument("--runs", type=int, default=5, help="dupehound runs per slice")
    ap.add_argument("--agent-runs", type=int, default=3, help="agent runs per slice")
    ap.add_argument("--agent-targets", type=int, nargs="*", default=None,
                    help="run agents only on these slice sizes (default: all targets)")
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--per-run-cap", type=float, default=8.0, help="Claude $/run cap")
    ap.add_argument("--max-turns", type=int, default=80, help="agent turns/iters per run")
    ap.add_argument("--agent-timeout", type=int, default=480, help="hard wall cap per run (s)")
    ap.add_argument("--date", default=None)
    ap.add_argument("--tag", default="ts-realistic")
    ap.add_argument("--results-dir", default=os.path.join(HERE, "..", "results"))
    args = ap.parse_args()

    date = args.date
    if date is None:
        import datetime
        date = datetime.date.today().isoformat()

    print("building slices...")
    host_meta = host.build_slices(args.repo, args.targets, args.seed, "ts",
                                  args.commit, args.profile)
    slices = sorted(host_meta["slices"], key=lambda s: s["host_loc"])

    dh_version, dh_rows = run_dupehound(slices, args.runs, args.threshold, args.min_tokens)

    agent_rows, spent, notes = [], 0.0, []
    models = [] if args.no_agent else args.models
    if not args.no_agent:
        agent_slices = slices
        if args.agent_targets:
            keep = set(args.agent_targets)
            agent_slices = [s for s in slices if s["target_loc"] in keep]
        agent_rows, spent, notes = run_agents(
            agent_slices, args.models, args.agent_runs, args.per_run_cap,
            args.max_turns, args.agent_timeout, args.concurrency)

    type_counts = load_gt(slices[0]).get("clone_type_counts", {})
    meta = {
        "date": date, "tag": args.tag, "repo": host_meta["repo"],
        "commit": host_meta["commit"], "seed": args.seed, "profile": args.profile,
        "clone_type_counts": type_counts,
        "slices": slices, "needle_functions": slices[0]["needle_functions"],
        "needle_pairs": slices[0]["needle_pairs"], "dupehound": dh_version,
        "models": models, "dh_runs": args.runs, "agent_runs": args.agent_runs,
        "spent": spent, "notes": notes,
    }

    os.makedirs(os.path.abspath(args.results_dir), exist_ok=True)
    base = os.path.join(os.path.abspath(args.results_dir), "%s-%s" % (date, args.tag))
    with open(base + ".json", "w") as f:
        json.dump({"meta": _meta_json(meta), "dupehound": _rows_json(dh_rows),
                   "agents": _rows_json(agent_rows)}, f, indent=2)
    write_markdown(base + ".md", meta, dh_rows, agent_rows)
    print("wrote %s.md and %s.json" % (base, base))
    print("agent token cost (OpenAI real $; Claude on subscription): ~$%.2f" % spent)


def _rows_json(rows):
    out = []
    for r in rows:
        rr = {k: v for k, v in r.items() if k != "predicted_pairs"}
        out.append(rr)
    return out


def _meta_json(meta):
    m = dict(meta)
    m["slices"] = [{k: s[k] for k in ("name", "host_loc", "host_files")} for s in meta["slices"]]
    return m


if __name__ == "__main__":
    main()
