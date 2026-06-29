"""Scoring for the planted-clone benchmark.

The evaluation unit is the unordered function pair. The universe is every pair
of candidate functions; the planted clone pairs are the positives and all other
pairs are the negatives. Each system's output is a set of predicted pairs, so
the confusion matrix, precision/recall/F1/MCC, and per-clone-type recall follow
directly. Because the corpus contains no duplication we did not plant, a
predicted pair that is not a planted pair is a genuine false positive.
"""
import math
from itertools import combinations


def universe(gt):
    ids = [f["id"] for f in gt["functions"]]
    return {frozenset(p) for p in combinations(ids, 2)}


def positives(gt):
    return {frozenset((p["a"], p["b"])): p["clone_type"] for p in gt["pairs"]}


def confusion(predicted, gt):
    uni = universe(gt)
    pos = set(positives(gt).keys())
    neg = uni - pos
    pred = set(predicted) & uni
    tp = len(pred & pos)
    fp = len(pred & neg)
    fn = len(pos - pred)
    tn = len(neg - pred)
    return tp, fp, fn, tn


def mcc(tp, fp, fn, tn):
    num = tp * tn - fp * fn
    den = math.sqrt((tp + fp) * (tp + fn) * (tn + fp) * (tn + fn))
    return num / den if den else 0.0


def metrics(predicted, gt):
    tp, fp, fn, tn = confusion(predicted, gt)
    precision = tp / (tp + fp) if (tp + fp) else 0.0
    recall = tp / (tp + fn) if (tp + fn) else 0.0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) else 0.0
    return {
        "tp": tp, "fp": fp, "fn": fn, "tn": tn,
        "precision": precision, "recall": recall, "f1": f1,
        "mcc": mcc(tp, fp, fn, tn),
        "recall_by_type": recall_by_type(predicted, gt),
    }


def recall_by_type(predicted, gt):
    pos = positives(gt)
    pred = set(predicted)
    out = {}
    for t in sorted(set(pos.values())):
        of_type = [pair for pair, ct in pos.items() if ct == t]
        hit = sum(1 for pair in of_type if pair in pred)
        out[str(t)] = {"recall": hit / len(of_type) if of_type else 0.0,
                       "hit": hit, "total": len(of_type)}
    return out


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else None


def std(xs):
    xs = [x for x in xs if x is not None]
    if len(xs) < 2:
        return 0.0
    m = sum(xs) / len(xs)
    return math.sqrt(sum((x - m) ** 2 for x in xs) / (len(xs) - 1))


def jaccard(a, b):
    a, b = set(a), set(b)
    if not a and not b:
        return 1.0
    return len(a & b) / len(a | b)


def determinism(run_pair_sets):
    """Mean pairwise Jaccard of the predicted-pair sets across runs (1.0 = identical)."""
    if len(run_pair_sets) < 2:
        return 1.0
    sims = [jaccard(run_pair_sets[i], run_pair_sets[j])
            for i in range(len(run_pair_sets))
            for j in range(i + 1, len(run_pair_sets))]
    return sum(sims) / len(sims)


def aggregate(runs, gt):
    """runs: list of system result dicts (same system, n runs). Returns aggregate."""
    per_run = [metrics(r["predicted_pairs"], gt) for r in runs]
    tel = [r["telemetry"] for r in runs]
    pair_sets = [r["predicted_pairs"] for r in runs]

    f1s = [m["f1"] for m in per_run]
    recs = [m["recall"] for m in per_run]
    precs = [m["precision"] for m in per_run]
    usds = [t.get("usd") for t in tel]
    walls = [t.get("wall_ms") for t in tel]

    tp_mean = mean([m["tp"] for m in per_run])
    usd_mean = mean(usds)
    cost_per_tp = (usd_mean / tp_mean) if (usd_mean and tp_mean) else (0.0 if usd_mean == 0.0 else None)

    # average per-type recall across runs
    types = sorted(per_run[0]["recall_by_type"].keys(), key=int) if per_run else []
    rbt = {t: mean([m["recall_by_type"][t]["recall"] for m in per_run]) for t in types}

    return {
        "system": runs[0]["system"],
        "kind": runs[0]["kind"],
        "simulated": runs[0].get("simulated", False),
        "runs": len(runs),
        "f1_mean": mean(f1s), "f1_std": std(f1s),
        "recall_mean": mean(recs), "recall_std": std(recs),
        "precision_mean": mean(precs), "precision_std": std(precs),
        "mcc_mean": mean([m["mcc"] for m in per_run]),
        "recall_by_type": rbt,
        "tp_mean": tp_mean,
        "fp_mean": mean([m["fp"] for m in per_run]),
        "fn_mean": mean([m["fn"] for m in per_run]),
        "wall_ms_mean": mean(walls), "wall_ms_std": std(walls),
        "tokens_in_mean": mean([t.get("tokens_in") for t in tel]),
        "tokens_out_mean": mean([t.get("tokens_out") for t in tel]),
        "usd_mean": usd_mean,
        "cost_per_tp": cost_per_tp,
        "determinism": determinism(pair_sets),
    }
