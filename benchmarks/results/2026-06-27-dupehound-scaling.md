# Scaling benchmark: finding planted duplicates in a large repo, 2026-06-27

> Same 19 planted duplicate pairs, hidden in growing slices of a real TypeScript codebase (microsoft/vscode @ `dac0f5b391`). Who still finds them as the haystack grows, and what does it cost? Method: [METHODOLOGY.md](../METHODOLOGY.md).

## Provenance

| field | value |
|---|---|
| host repo | microsoft/vscode @ `dac0f5b391` |
| slices (host LOC) | 10071, 100101, 500023 |
| planted needles | 32 functions, 19 clone pairs (seed 7) |
| dupehound | `dupehound 0.1.0` |
| agent models | none |
| dupehound runs / agent runs | 2 / 1 |
| agent budget | spent $0.00 of $50.00 cap |

## Scoreboard by repo size

Each cell: duplicates found of 19, with time and cost per run.

| system | repo size (LOC) | duplicates found | false alarms | time / run | cost / run |
|---|---|---|---|---|---|
| dupehound 0.1.0 | 10,071 | 15 of 19 | 0 | 30 ms | $0 |
| dupehound 0.1.0 | 100,101 | 15 of 19 | 0 | 85 ms | $0 |
| dupehound 0.1.0 | 500,023 | 15 of 19 | 0 | 328 ms | $0 |

## What it costs to hold recall as the repo grows

| system | 10k | 100k | 500k |
|---|---|---|---|
| dupehound 0.1.0 | 30ms / $0 | 85ms / $0 | 328ms / $0 |

## Recall by clone type (largest slice)

| system | copy-paste (T1) | renamed (T2) | edited (T3) | rewritten (T4) |
|---|---|---|---|---|
| dupehound 0.1.0 | 1.000 | 1.000 | 0.800 | 0.000 |

---
Regenerate: `python3 harness/run_scaling.py --tag dupehound-scaling`
