#!/usr/bin/env python3
"""Benchmark orchestrator.

Generates (or reuses) a planted-clone corpus, runs every system n times,
scores each against ground truth, and writes a dated results pair:

    results/<date>-<tag>.json   full machine-readable record
    results/<date>-<tag>.md     human-readable tables + provenance

Determinism note: dupehound is run n times too, so its run-to-run stability is
measured, not assumed.

Examples:
    # deterministic tier only (free, runs in CI):
    python3 run.py --no-llm --tag dupehound-only --date 2026-06-26

    # full tier with the simulated LLM panel (proves the pipeline, no spend):
    python3 run.py --models mock:frontier-a mock:frontier-b mock:frontier-c mock:cheap-open \
        --runs 5 --tag demo-sim --date 2026-06-26

    # full tier with real models (needs API keys + prices set in systems/llm.py):
    python3 run.py --models claude-opus-4-8 gpt-frontier --runs 5 --tag real
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import paths  # noqa: E402
import plant  # noqa: E402
import score  # noqa: E402
from systems import dupehound as dh_sys  # noqa: E402
from systems import llm as llm_sys  # noqa: E402


def load_or_build_corpus(corpus_dir, seed):
    gt_path = os.path.join(corpus_dir, "ground_truth.json")
    if not os.path.exists(gt_path):
        plant.build(corpus_dir, seed)
    with open(gt_path) as f:
        gt = json.load(f)
    if gt.get("seed") != seed:
        plant.build(corpus_dir, seed)
        with open(gt_path) as f:
            gt = json.load(f)
    return gt


def run_system_dupehound(corpus_dir, gt, runs, threshold, min_tokens):
    results = []
    for i in range(runs):
        results.append(dh_sys.run(corpus_dir, gt, threshold, min_tokens))
    return results


def run_system_llm(corpus_dir, gt, model, runs):
    return [llm_sys.run(corpus_dir, gt, model, run_idx=i) for i in range(runs)]


def fmt(v, spec="{:.3f}", dash="n/a"):
    return dash if v is None else spec.format(v)


def usd_fmt(v):
    if v is None:
        return "n/a"
    if v == 0:
        return "$0"
    if v < 0.01:
        return "${:.5f}".format(v)
    return "${:.4f}".format(v)


def time_fmt(ms):
    if ms is None:
        return "n/a"
    if ms < 1000:
        return "%d ms" % round(ms)
    return "%.1f s" % (ms / 1000.0)


def same_answer(determinism):
    if determinism is None:
        return "n/a"
    if determinism >= 0.999:
        return "yes (identical)"
    return "no (~%d%% overlap)" % round(determinism * 100)


def write_markdown(path, gt, aggs, meta):
    n = len(gt["functions"])
    lines = []
    lines.append("# Duplication detection benchmark -- %s" % meta["date"])
    lines.append("")
    lines.append("> Hide-and-seek: known duplicates are planted in real source; "
                 "each system is scored on what it recovers, how fast, how cheap, "
                 "and how reliably. Method: [METHODOLOGY.md](../METHODOLOGY.md).")
    lines.append("")
    if any(a.get("simulated") for a in aggs):
        lines.append("> **SIMULATED LLM ARMS.** Rows marked *(sim)* use the mock "
                     "backend, not real model calls. They validate the harness and "
                     "illustrate the expected frontier; replace with real keys + "
                     "published prices for citable numbers.")
        lines.append("")

    lines.append("## Provenance")
    lines.append("")
    lines.append("| field | value |")
    lines.append("|---|---|")
    lines.append("| corpus | `%s` (seed %d) |" % (gt["host"], gt["seed"]))
    lines.append("| functions | %d |" % n)
    type_counts = " ".join("T%s:%s" % (t, c) for t, c in sorted(gt["clone_type_counts"].items()))
    lines.append("| planted clone pairs | %d  (%s) |" % (len(gt["pairs"]), type_counts))
    lines.append("| candidate pairs (universe) | %d |" % (n * (n - 1) // 2))
    lines.append("| dupehound | `%s` |" % meta["dupehound"])
    lines.append("| runs per system (n) | %d |" % meta["runs"])
    lines.append("| dupehound config | threshold %.2f, min-tokens %d |"
                 % (meta["threshold"], meta["min_tokens"]))
    lines.append("| token counting | %s |" % meta["token_note"])
    lines.append("")

    total_pairs = len(gt["pairs"])

    lines.append("## Scoreboard")
    lines.append("")
    lines.append("We hid **%d** real duplicate pairs in the code. Each row: how many "
                 "of them the system found, how often it cried wolf, how long it took, "
                 "what it cost, and whether it gives the same answer twice." % total_pairs)
    lines.append("")
    lines.append("| system | duplicates found | false alarms | time / run | cost / run | same answer every run? |")
    lines.append("|---|---|---|---|---|---|")
    for a in aggs:
        name = a["system"] + (" *(sim)*" if a.get("simulated") else "")
        found = "%d of %d" % (round(a["tp_mean"]), total_pairs)
        false_alarms = "%d" % round(a["fp_mean"])
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            name, found, false_alarms,
            time_fmt(a["wall_ms_mean"]),
            usd_fmt(a["usd_mean"]),
            same_answer(a["determinism"]),
        ))
    lines.append("")
    lines.append("*False alarms* = pairs it flagged that are not really duplicates. "
                 "*Same answer every run?* = run it five times, does it return the "
                 "identical set? A deterministic tool does; an LLM usually does not.")
    lines.append("")

    lines.append("## What kinds of duplicate it catches")
    lines.append("")
    lines.append("Type-1 = copy-paste, Type-2 = copy-paste then renamed, Type-3 = "
                 "renamed with a few lines changed, Type-4 = same idea rewritten "
                 "differently. dupehound is built for Type-1/2; Type-3/4 are included "
                 "on purpose to show, honestly, where an LLM earns its cost.")
    lines.append("")
    types = ["1", "2", "3", "4"]
    type_total = {t: gt["clone_type_counts"].get(t, 0) for t in types}
    lines.append("| system | " + " | ".join(
        "copy-paste (T1)" if t == "1" else
        "renamed (T2)" if t == "2" else
        "edited (T3)" if t == "3" else "rewritten (T4)" for t in types) + " |")
    lines.append("|---|" + "|".join("---" for _ in types) + "|")
    for a in aggs:
        name = a["system"] + (" *(sim)*" if a.get("simulated") else "")
        cells = []
        for t in types:
            r = a["recall_by_type"].get(t)
            tot = type_total[t]
            cells.append("%d of %d" % (round((r or 0) * tot), tot) if tot else "-")
        lines.append("| %s | %s |" % (name, " | ".join(cells)))
    lines.append("")

    lines.append("## Detailed metrics (for the report)")
    lines.append("")
    lines.append("Glossary: **recall** = share of hidden duplicates found; "
                 "**precision** = share of flagged pairs that are real; **F1** = "
                 "single balanced score of the two; **MCC** = balanced score that "
                 "also rewards correct non-matches (1.0 perfect, 0 random). "
                 "**$/true-positive** = cost per real duplicate found.")
    lines.append("")
    lines.append("| system | recall | precision | F1 | MCC | $/true-positive | tokens in/out |")
    lines.append("|---|---|---|---|---|---|---|")
    for a in aggs:
        name = a["system"] + (" *(sim)*" if a.get("simulated") else "")
        toks = "%s / %s" % (fmt(a["tokens_in_mean"], "{:.0f}"), fmt(a["tokens_out_mean"], "{:.0f}"))
        lines.append("| %s | %s | %s | %s | %s | %s | %s |" % (
            name,
            fmt(a["recall_mean"]),
            fmt(a["precision_mean"]),
            "%s ±%s" % (fmt(a["f1_mean"]), fmt(a["f1_std"], "{:.2f}")),
            fmt(a["mcc_mean"]),
            usd_fmt(a["cost_per_tp"]),
            toks,
        ))
    lines.append("")

    lines.append("---")
    lines.append("Regenerate: `python3 harness/run.py --date %s --tag %s`"
                 % (meta["date"], meta["tag"]))
    lines.append("")

    with open(path, "w") as f:
        f.write("\n".join(lines))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--threshold", type=float, default=0.80)
    ap.add_argument("--min-tokens", type=int, default=20)
    ap.add_argument("--models", nargs="*", default=[
        "mock:frontier-a", "mock:frontier-b", "mock:frontier-c", "mock:cheap-open"])
    ap.add_argument("--no-llm", action="store_true")
    ap.add_argument("--date", default=None)
    ap.add_argument("--tag", default="demo")
    ap.add_argument("--corpus", default=os.path.join(paths.data_dir(), "synthetic-py-v1"))
    ap.add_argument("--results-dir", default=os.path.join(HERE, "..", "results"))
    args = ap.parse_args()

    date = args.date
    if date is None:
        import datetime
        date = datetime.date.today().isoformat()

    corpus = os.path.abspath(args.corpus)
    gt = load_or_build_corpus(corpus, args.seed)

    aggs = []
    raw = {}

    dh_runs = run_system_dupehound(corpus, gt, args.runs, args.threshold, args.min_tokens)
    aggs.append(score.aggregate(dh_runs, gt))
    raw[dh_runs[0]["system"]] = _serializable(dh_runs)
    dupehound_version = dh_runs[0]["system"]

    if not args.no_llm:
        for model in args.models:
            runs = run_system_llm(corpus, gt, model, args.runs)
            aggs.append(score.aggregate(runs, gt))
            raw[model] = _serializable(runs)

    meta = {
        "date": date, "tag": args.tag, "runs": args.runs,
        "threshold": args.threshold, "min_tokens": args.min_tokens,
        "dupehound": dupehound_version,
        "token_note": "real backends use provider usage; mock + fallback use chars/4",
    }

    os.makedirs(os.path.abspath(args.results_dir), exist_ok=True)
    base = os.path.join(os.path.abspath(args.results_dir), "%s-%s" % (date, args.tag))
    with open(base + ".json", "w") as f:
        json.dump({"meta": meta, "ground_truth": _gt_summary(gt),
                   "aggregates": aggs, "raw": raw}, f, indent=2)
    write_markdown(base + ".md", gt, aggs, meta)

    print("wrote %s.md and %s.json" % (base, base))
    for a in aggs:
        print("  %-22s F1=%s recall=%s prec=%s $/run=%s det=%s" % (
            a["system"], fmt(a["f1_mean"]), fmt(a["recall_mean"]),
            fmt(a["precision_mean"]), usd_fmt(a["usd_mean"]), fmt(a["determinism"])))


def _serializable(runs):
    out = []
    for r in runs:
        rr = dict(r)
        rr["predicted_pairs"] = [sorted(p) for p in r["predicted_pairs"]]
        out.append(rr)
    return out


def _gt_summary(gt):
    return {k: gt[k] for k in ("seed", "host", "language", "groups",
                               "clone_type_counts", "gap_profile") if k in gt}


if __name__ == "__main__":
    main()
