# Scaling benchmark: finding planted duplicates in a large repo, 2026-06-28

> Same 39 planted duplicate pairs, hidden in growing slices of a real TypeScript codebase (microsoft/vscode @ `dac0f5b391`). Who still finds them as the haystack grows, and what does it cost? Method: [METHODOLOGY.md](../METHODOLOGY.md).

## Provenance

| field | value |
|---|---|
| host repo | microsoft/vscode @ `dac0f5b391` |
| slices (host LOC) | 10071, 100101, 500023, 1000017 |
| duplication profile | realistic (T1:3 T2:30 T3:3 T4:3) |
| planted needles | 42 functions, 39 clone pairs (seed 7) |
| dupehound | `dupehound 0.1.0` |
| agent models | haiku, sonnet, opus |
| dupehound runs / agent runs | 5 / 2 |
| agent token cost | ~$29.59 (OpenAI real $; Claude on Max subscription) |
| harness | Claude via `claude -p` (its own agent loop); OpenAI via our Python tool-loop (Glob/Grep/Read) |

## Scoreboard by repo size

Each cell: duplicates found of 39, with time and cost per run.

| system | repo size (LOC) | duplicates found | false alarms | time / run | tokens in/out | cost / run |
|---|---|---|---|---|---|---|
| claude-agent:haiku | 10,071 | 20 of 39 | 0 | 205.2 s | 1146k / 20k | $0.3351 |
| claude-agent:haiku | 100,101 | 16 of 39 | 0 | 194.4 s | 2446k / 14k | $0.4444 |
| claude-agent:haiku | 1,000,017 | 6 of 39 | 0 | 275.9 s | 4120k / 17k | $0.6528 |
| claude-agent:opus | 10,071 | 22 of 39 | 0 | 317.5 s | 200k / 28k | $1.4657 |
| claude-agent:opus | 100,101 | 13 of 39  (1 run hung) | 0 | 539.9 s | 652k / 45k | $2.6094 |
| claude-agent:opus | 1,000,017 | 0 of 39 | 0 | 819.9 s | 721k / 35k | $2.5374 |
| claude-agent:sonnet | 10,071 | 19 of 39 | 0 | 339.2 s | 335k / 24k | $0.6808 |
| claude-agent:sonnet | 100,101 | 0 of 39  (1 run hung) | 0 | 532.1 s | 6195k / 26k | $3.3837 |
| claude-agent:sonnet | 1,000,017 | DNF (hung) | 0 | n/a | n/a / n/a | n/a |
| dupehound 0.1.0 | 10,071 | 36 of 39 | 0 | 29 ms | 0 / 0 | $0 |
| dupehound 0.1.0 | 100,101 | 36 of 39 | 0 | 89 ms | 0 / 0 | $0 |
| dupehound 0.1.0 | 500,023 | 36 of 39 | 0 | 370 ms | 0 / 0 | $0 |
| dupehound 0.1.0 | 1,000,017 | 36 of 39 | 0 | 743 ms | 0 / 0 | $0 |

*DNF / "run hung" = an agent run timed out or was killed without finishing; its recall is not a real "found zero", just an unfinished exploration. dupehound never hangs.*

## What it costs to hold recall as the repo grows

| system | 10,071 | 100,101 | 500,023 | 1,000,017 |
|---|---|---|---|---|
| claude-agent:haiku | 205.2 s / $0.3351 | 194.4 s / $0.4444 | - | 275.9 s / $0.6528 |
| claude-agent:opus | 317.5 s / $1.4657 | 539.9 s / $2.6094 | - | 819.9 s / $2.5374 |
| claude-agent:sonnet | 339.2 s / $0.6808 | 532.1 s / $3.3837 | - | n/a / n/a |
| dupehound 0.1.0 | 29 ms / $0 | 89 ms / $0 | 370 ms / $0 | 743 ms / $0 |

## Recall by clone type, per model and repo size

How many of each kind each system recovered. T1 = exact copy-paste, T2 = renamed copy-paste (the bulk of real duplication), T3 = renamed + a few edited lines, T4 = same behaviour rewritten differently. Totals planted: T1=3 T2=30 T3=3 T4=3.

| system | repo size | copy-paste (T1) | renamed (T2) | edited (T3) | rewritten (T4) |
|---|---|---|---|---|---|
| claude-agent:haiku | 10,071 | 3/3 | 16/30 | 0/3 | 0/3 |
| claude-agent:haiku | 100,101 | 2/3 | 13/30 | 2/3 | 0/3 |
| claude-agent:haiku | 1,000,017 | 0/3 | 4/30 | 0/3 | 2/3 |
| claude-agent:opus | 10,071 | 3/3 | 18/30 | 0/3 | 0/3 |
| claude-agent:opus | 100,101 | 3/3 | 9/30 | 1/3 | 0/3 |
| claude-agent:opus | 1,000,017 | 0/3 | 0/30 | 0/3 | 0/3 |
| claude-agent:sonnet | 10,071 | 3/3 | 16/30 | 0/3 | 0/3 |
| claude-agent:sonnet | 100,101 | 0/3 | 0/30 | 0/3 | 0/3 |
| claude-agent:sonnet | 1,000,017 | DNF | DNF | DNF | DNF |
| dupehound 0.1.0 | 10,071 | 3/3 | 30/30 | 3/3 | 0/3 |
| dupehound 0.1.0 | 100,101 | 3/3 | 30/30 | 3/3 | 0/3 |
| dupehound 0.1.0 | 500,023 | 3/3 | 30/30 | 3/3 | 0/3 |
| dupehound 0.1.0 | 1,000,017 | 3/3 | 30/30 | 3/3 | 0/3 |

## Notes

- sonnet on microsoft_vscode-100k run 1: did not finish (error_max_turns)
- opus on microsoft_vscode-100k run 1: agent timed out after 900s
- sonnet on microsoft_vscode-1m run 1: did not finish (error_max_turns)
- sonnet on microsoft_vscode-1m run 0: agent timed out after 900s
- sonnet on microsoft_vscode-1m: all runs failed (DNF)

---
Regenerate: `python3 harness/run_scaling.py --tag ts-final-claude`
