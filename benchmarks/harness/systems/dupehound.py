"""dupehound system adapter for the benchmark.

Runs the dupehound binary over a planted corpus, maps every reported cluster
member back to a ground-truth function, and expands clusters into predicted
clone pairs. Telemetry: wall-clock measured here; tokens and cost are zero by
construction (local, deterministic CPU work).
"""
import json
import os
import subprocess
import time


def _bin():
    env = os.environ.get("DUPEHOUND_BIN")
    if env:
        return env
    here = os.path.dirname(__file__)
    return os.path.abspath(os.path.join(here, "..", "..", "..", "target", "release", "dupehound"))


def _version(binpath):
    try:
        out = subprocess.run([binpath, "--version"], capture_output=True, text=True)
        return out.stdout.strip() or "dupehound"
    except Exception:
        return "dupehound"


def _index_ground_truth(gt):
    """(basename, function name) -> func_id.

    Keyed by basename + function name: needle files may also contain sub-threshold
    filler, so we identify the planted function by name rather than assuming one
    function per file. dupehound reports paths relative to its scan root while
    ground truth records them relative to the corpus root, so basename is used.
    """
    return {(os.path.basename(fn["file"]), fn["name"]): fn["id"]
            for fn in gt["functions"]}


def _match(index, member):
    """Map a reported member to a ground-truth function id by (basename, name)."""
    return index.get((os.path.basename(member["file"]), member["name"]))


def run(corpus_dir, gt, threshold=0.80, min_tokens=20):
    binpath = _bin()
    by_file = _index_ground_truth(gt)

    t0 = time.perf_counter()
    proc = subprocess.run(
        [binpath, "scan", corpus_dir, "--json",
         "--threshold", str(threshold),
         "--min-tokens", str(min_tokens),
         "--include-tests"],
        capture_output=True, text=True,
    )
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    if proc.returncode != 0:
        raise RuntimeError("dupehound failed: %s" % proc.stderr[:500])
    report = json.loads(proc.stdout)

    predicted = set()
    for cluster in report.get("clusters", []):
        ids = []
        for m in cluster["members"]:
            fid = _match(by_file, m)
            if fid is not None:
                ids.append(fid)
        for i in range(len(ids)):
            for j in range(i + 1, len(ids)):
                if ids[i] != ids[j]:
                    predicted.add(frozenset((ids[i], ids[j])))

    return {
        "system": _version(binpath),
        "kind": "deterministic",
        "predicted_pairs": predicted,
        "telemetry": {
            "wall_ms": elapsed_ms,
            "self_reported_ms": report.get("stats", {}).get("elapsed_ms"),
            "tokens_in": 0,
            "tokens_out": 0,
            "usd": 0.0,
            "functions_seen": report.get("stats", {}).get("functions"),
        },
        "config": {"threshold": threshold, "min_tokens": min_tokens},
    }
