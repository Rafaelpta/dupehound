#!/usr/bin/env python3
"""Deterministic-tier regression gate (free, runs in CI on every change).

Builds the synthetic corpus, runs dupehound twice, and asserts the structural
invariants that must always hold. A code change that breaks any of them fails
the build. No network, no LLM, no spend.

Invariants:
  - precision == 1.0          (no false positives: the structural guarantee)
  - Type-1 recall == 1.0      (exact clones are never missed)
  - Type-2 recall == 1.0      (renamed clones are never missed)
  - determinism == 1.0        (identical output across runs)
  - F1 >= F1_FLOOR            (guards against gross regressions)

Run: DUPEHOUND_BIN=target/release/dupehound python3 harness/ci_check.py
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import paths      # noqa: E402
import plant      # noqa: E402
import score      # noqa: E402
from systems import dupehound as dh_sys  # noqa: E402

SEED = 7
THRESHOLD = 0.80
MIN_TOKENS = 20
F1_FLOOR = 0.70


def main():
    corpus = os.path.join(paths.data_dir(), "synthetic-py-v1")
    gt = plant.build(corpus, SEED)

    runs = [dh_sys.run(corpus, gt, THRESHOLD, MIN_TOKENS) for _ in range(2)]
    agg = score.aggregate(runs, gt)

    checks = [
        ("precision == 1.0", agg["precision_mean"] == 1.0, agg["precision_mean"]),
        ("Type-1 recall == 1.0", agg["recall_by_type"].get("1") == 1.0, agg["recall_by_type"].get("1")),
        ("Type-2 recall == 1.0", agg["recall_by_type"].get("2") == 1.0, agg["recall_by_type"].get("2")),
        ("determinism == 1.0", agg["determinism"] == 1.0, agg["determinism"]),
        ("F1 >= %.2f" % F1_FLOOR, agg["f1_mean"] >= F1_FLOOR, agg["f1_mean"]),
    ]

    print("dupehound deterministic-tier gate (%s)" % runs[0]["system"])
    print("  corpus: %d functions, %d planted pairs" % (len(gt["functions"]), len(gt["pairs"])))
    failed = False
    for label, ok, val in checks:
        print("  [%s] %-22s  (got %s)" % ("PASS" if ok else "FAIL", label, val))
        failed = failed or not ok

    print("  F1=%.3f precision=%.3f recall=%.3f det=%.3f"
          % (agg["f1_mean"], agg["precision_mean"], agg["recall_mean"], agg["determinism"]))
    if failed:
        print("REGRESSION: a structural invariant broke.")
        sys.exit(1)
    print("OK")


if __name__ == "__main__":
    main()
