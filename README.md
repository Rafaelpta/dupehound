<p align="center">
  <img src="assets/demo-card.png" alt="dupehound history card: duplication went from ~0 to 36.1% since 2025-05" width="720">
</p>

<h1 align="center">dupehound</h1>

<p align="center"><b>Finds the code your AI wrote twice.</b><br>
Fast, offline, deterministic. No API keys, no AI, no code leaves your machine.</p>

<p align="center">
  <a href="https://github.com/GITHUB_USER/dupehound/actions/workflows/ci.yml"><img src="https://github.com/GITHUB_USER/dupehound/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
  <img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="Rust">
</p>

AI coding agents can't hold a whole repo in context, so they re-implement code that already exists. `formatDate` becomes `renderTimestamp`, then `stringifyDate`, then `humanizeDate`. Same logic, four names, four places to fix the next bug. Analyses of millions of commits report that code duplication roughly doubled since AI assistants went mainstream.

dupehound fingerprints the structure of every function in your codebase, so renamed variables and changed literals can't hide a copy. It tells you what exists more than once, when the duplication started, and blocks new copies in CI.

```
$ dupehound scan .

  dupehound v0.1.0 — scanned 19 files · 370 lines · 27 functions in 21ms

  ╭─────────────────────────────────────────────────────────╮
  │  SLOP SCORE   36.1%   grade F                           │
  │  127 of 352 significant lines are deletable duplicates  │
  ╰─────────────────────────────────────────────────────────╯

  ● Cluster 1 ─ 4 copies · 100% similar · 42 deletable lines ─────────────
    ★ src/utils/date.ts:1        formatDate        14 lines
      src/api/timestamps.ts:1    renderTimestamp   14 lines   100% █████████
      src/jobs/report_dates.ts:1 stringifyDate     14 lines   100% █████████
      src/billing/dates.ts:1     humanizeDate      14 lines   100% █████████

  ★ = representative (kept) · dupehound scan --explain 1 shows the code
```

Supports TypeScript, TSX, JavaScript, Python, Rust, Go and Java, in any mix.

## The three commands

### dupehound scan

Finds clusters of near-duplicate functions across the repo, even when every identifier and literal was renamed. Ranks them by deletable lines and computes the slop score:

> slop score = the percentage of your code you could delete if every duplicate cluster kept only one copy.

That sentence is the entire formula. The biggest copy in each cluster is exempt, since the original isn't the problem. Test files don't count by default, because table-driven tests are legitimately repetitive. Use `--json` for scripts, `--explain N` to print a cluster's code as proof, `--card` for a shareable score card.

### dupehound history

Replays your git history (one snapshot per month, no checkouts, no temp dirs), charts duplication over time, and finds the inflection point:

```
$ dupehound history .

   36.1% ┤                      ██
         ┤                  ▂▂▆▆██
         ┤              ▂▂████████
         ┤          ▁▁████████████
    0.0% ┤          ██████████████
         └────────────────────────
          2025-01          2025-12

  current slop score: 36.1% (grade F)
  duplication went from ~0 to 36.1% since 2025-05

  card written → dupehound-card.svg / dupehound-card.png
```

It also writes the card at the top of this README as SVG and PNG. dupehound only measures duplication. It never claims to know who or what wrote the code.

### dupehound check

The CI and pre-commit gate. It indexes the codebase at your base revision, then probes only the functions your change adds or touches:

```
$ dupehound check --diff main .
src/api/orders.ts:1 calculateOrderAmount() is a 100% duplicate of src/billing/invoice.ts:1 computeInvoiceTotal() — reuse it

dupehound check: 1 new duplicate of existing code
$ echo $?
1
```

Moved functions and in-place edits are recognized and never fire. These are the two classic CI false alarms. Output is one line per finding, readable by humans and by coding agents (see below).

## Install

Prebuilt binaries for macOS, Linux and Windows are on the [releases page](https://github.com/GITHUB_USER/dupehound/releases).

With cargo:

```sh
cargo install dupehound
```

From source:

```sh
git clone https://github.com/GITHUB_USER/dupehound && cd dupehound
cargo build --release   # binary at target/release/dupehound
```

`history` and `check` need `git` on PATH. Plain `scan` works anywhere, repo or not.

## How it works

There is no machine learning anywhere in the pipeline:

1. **Parse.** Every file goes through [tree-sitter](https://tree-sitter.github.io/). Function and method bodies are the unit of comparison, so imports, signatures and license headers can never match.
2. **Normalize.** Identifiers become `ID`, string literals `STR`, numbers `NUM`, comments are dropped. Keywords, operators and structure stay. A fully renamed copy (a type-2 clone) now produces an identical token stream.
3. **Fingerprint.** Rolling-hashed k-grams (k=10) are selected by robust winnowing, the algorithm behind Stanford's MOSS plagiarism detector ([Schleimer, Wilkerson & Aiken, SIGMOD 2003](https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf)). Winnowing guarantees that any shared run of 17 or more normalized tokens is caught, and nothing shorter than 10 tokens can ever match. The test suite asserts both properties.
4. **Match.** An inverted fingerprint index generates candidate pairs, so there is no O(n²) all-pairs pass. Fingerprints shared by too many functions (language boilerplate like Go's `if err != nil` ladders) are culled before they can pair everything with everything. Similarity is exact Jaccard over fingerprint sets, and union-find groups pairs into clusters.
5. **Score.** The slop score formula above, printed under the score on every run.

Insertions and deletions only disturb fingerprints near the edit, so near-miss (type-3) clones surface below the default 80% similarity bar. Lower `--threshold` to dig.

## Why not just ask an LLM?

Fair question, since an LLM probably wrote the duplicates.

- **Exhaustiveness.** Finding duplicates means comparing every function against every other, effectively billions of comparisons in a large repo. A model samples what fits in context. An index checks everything, every time.
- **Determinism.** A CI gate that blocks merges must be reproducible and auditable: same input, same verdict, an algorithm you can read.
- **Cost and speed.** dupehound scans about 3M lines in under 4 seconds, locally, for free, on every commit.
- **Privacy.** Your code never leaves the machine. No API key to configure or leak.

## False positives

A duplicate detector lives or dies on false positives, so the defaults are conservative:

- Test files are scanned and labeled `[tests, not scored]` but excluded from the slop score. Override with `--include-tests` or `--exclude-tests`. Rust inline `#[cfg(test)] mod` blocks are detected too.
- Generated and vendored code is skipped: `@generated` and `DO NOT EDIT` markers, `*.pb.go`, `*_pb2.py`, `*.min.js`, `*.d.ts`, and directories like `node_modules/`, `vendor/`, `dist/`, `target/`, even when not gitignored. Override with `--no-default-excludes`.
- Tiny functions can't match. The default minimum is 40 normalized tokens (`--min-tokens`), so getters and one-line builders don't flood the report.
- Every match is verifiable in one command: `dupehound scan --explain N` prints the cluster's code side by side.

## Calibration

Grades are calibrated against well-maintained open source repos (scanned June 2026):

| Repo | Language | Lines | Slop score | Grade |
|---|---|---:|---:|:-:|
| expressjs/express | JavaScript | 21k | 0.0% | A |
| gin-gonic/gin | Go | 24k | 0.2% | A |
| tokio-rs/tokio | Rust | 175k | 1.1% | A |
| tiangolo/fastapi | Python | 109k | 1.7% | A |
| microsoft/vscode | TypeScript | 2.97M | 2.8% | A |

A < 3%, B < 6%, C < 10%, D < 15%, F ≥ 15%. Healthy, human-curated codebases land in A. If your repo doesn't, the report shows exactly which functions to merge.

## Performance

Single binary, parallel from disk to report. Measured on an M-series laptop:

| Codebase | Lines | Functions | Time |
|---|---:|---:|---:|
| tokio | 175k | 3,137 | 0.12s |
| fastapi | 109k | 1,758 | 0.19s |
| vscode | 2,970,884 | 53,375 | 3.6s |

`history` caches fingerprints by blob SHA across snapshots, so charting 36 months costs a few times one scan, not 36 times.

## CI recipe

```yaml
# .github/workflows/dupehound.yml
name: dupehound
on: pull_request
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0   # check needs the merge-base
      - name: Install dupehound
        run: |
          curl -sL https://github.com/GITHUB_USER/dupehound/releases/latest/download/dupehound-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv dupehound /usr/local/bin/
      - name: Block new duplicates
        run: dupehound check --diff origin/${{ github.base_ref }} .
```

## Using with AI agents

The `check` output is designed to be fed back to the agent that caused it. Add this to your `CLAUDE.md` or `AGENTS.md`:

```markdown
Before committing, run `dupehound check .`. If it reports that a function
you wrote duplicates existing code, delete your version and reuse the
original at the reported location.
```

The agent gets a queryable memory of what already exists, and duplicates stop at the door.

## Roadmap

- More languages: C/C++, C#, Ruby, PHP, Kotlin, Swift (each one is roughly a query file, see [CONTRIBUTING.md](CONTRIBUTING.md))
- Containment matching (a small function copied into a big one)
- `--fix` suggestions: the import that replaces a duplicate
- Editor integrations

## License

MIT. Bundled [JetBrains Mono](https://www.jetbrains.com/lp/mono/) subsets are under the [SIL OFL 1.1](assets/fonts/OFL.txt).
