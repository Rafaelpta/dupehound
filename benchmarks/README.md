# dupehound benchmark

A hide-and-seek benchmark for code-duplication detection: known duplicates are
**planted** in real source, and every system is scored on what it recovers, how
fast, how cheap, and how reliably.

- **Read the result:** [results/2026-06-28-agents-vs-dupehound.md](results/2026-06-28-agents-vs-dupehound.md), dupehound vs Claude agents on `microsoft/vscode`, 10k to 1,000,017 LOC.
- **Method and threats to validity:** [METHODOLOGY.md](METHODOLOGY.md).
- **Reproduce:** see below. Everything is deterministic from a fixed seed against a pinned commit.

## What it does

`harness/host.py` plants a fixed set of duplicate functions (39 pairs, mostly
renamed copy-paste) into growing slices of a real TypeScript repository, and
`run_scaling.py` scores each system on what it recovers, with full per-run
telemetry. dupehound runs deterministically for free. The Claude agents run
headless (`claude -p`) with read-only file tools (glob, grep, read) and
isolation: no MCP servers (so an agent cannot call dupehound), no shell, no
sub-agents, and no access to the ground-truth file.

## Layout

```
benchmarks/
  METHODOLOGY.md          method and threats to validity
  results/                dated result files (md + json) and the writeup
  harness/
    plant.py              deterministic planted-clone generator (--lang python|ts)
    host.py               clone + slice microsoft/vscode, plant needles
    systems/dupehound.py  runs dupehound, maps its output to clone pairs
    systems/agent.py      Claude agent arm (headless claude -p)
    systems/api_agent.py  OpenAI agent arm (a tool loop with the same tools)
    run_scaling.py        the scaling-curve orchestrator
    granular.py           per-run telemetry reconstruction
    score.py              pair-level scoring (precision, recall, by clone type)
    ci_check.py           free deterministic regression gate (runs in CI)
```

Generated corpora live outside the repo (default `$TMPDIR/dupehound-bench`,
override with `DUPEHOUND_BENCH_DATA`), so the scanned trees are never committed.

## Reproduce

Build dupehound (`cargo build --release`), then from `benchmarks/harness`:

```bash
# dupehound on every slice (deterministic, free)
DUPEHOUND_BIN=../../target/release/dupehound python3 run_scaling.py --no-agent

# the Claude agent grid (needs the `claude` CLI logged in)
DUPEHOUND_BIN=../../target/release/dupehound python3 run_scaling.py \
    --profile realistic --models haiku sonnet opus \
    --agent-targets 10000 100000 1000000 --agent-runs 2 \
    --concurrency 2 --max-turns 150 --agent-timeout 900

# per-run telemetry (tokens, time, recovery for each individual run)
python3 granular.py
```

Same `--seed` (default 7) always yields the same planted corpus and ground
truth. A citable run pins the dupehound commit, the seed, and the model snapshot
ids. The `agents-vs-dupehound.md` writeup records this provenance.
