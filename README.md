<p align="center">
  <img src="assets/hound.png" alt="dupehound" width="200">
</p>

<h1 align="center">dupehound</h1>

<p align="center">Finds functions duplicated by AI, even after every identifier is renamed.</p>

<p align="center">
  <a href="./LICENSE"><img alt="Open Source" src="https://img.shields.io/badge/open%20source-100%25-success"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue">
  <a href="https://github.com/Rafaelpta/dupehound/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/Rafaelpta/dupehound/actions/workflows/ci.yml/badge.svg"></a>
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/github/license/Rafaelpta/dupehound?color=blue"></a>
  <a href="https://github.com/Rafaelpta/dupehound/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/Rafaelpta/dupehound"></a>
</p>

dupehound is a duplicate-code detector built for codebases where agents write most of the code. It finds functions that exist more than once, even after every identifier and literal has been renamed, because it fingerprints the structure of the code instead of its text.

| Command | What it does |
|---------|--------------|
| `scan` | reports every duplicate cluster and a repo-level slop score |
| `history` | charts duplication across the git log and pinpoints when it took off |
| `check` | fails CI when a change duplicates code that already exists, naming the original to reuse |

Everything runs locally and deterministically (no network, API keys, or AI required).

## Numbers

We planted 39 known duplicate function pairs (mostly renamed copy-paste, the bulk of real duplication) in real code from [microsoft/vscode](https://github.com/microsoft/vscode), a 3.3-million-line TypeScript codebase, and grew the host from 10,000 to 1,000,017 lines. Each system was scored on what it recovers: dupehound, and three Claude agents (Haiku 4.5, Sonnet 4.6, Opus 4.8) given the same read-only file tools (glob, grep, read), two runs per model per size. Exact host sizes and the full repository breakdown are in the [writeup](benchmarks/results/2026-06-28-agents-vs-dupehound.md).

<p align="center">
  <img src="assets/benchmark-scaling.svg" width="820" alt="Duplicate pairs recovered versus repository size. dupehound holds 36 of 39 flat from 10k to 1M lines. Claude Opus falls 22, 13, 0. Claude Haiku falls 20, 16, 6. Claude Sonnet falls 19, 0, then times out.">
</p>

| pairs recovered (of 39) | 10k LOC | 100k LOC | 1M LOC | time at 1M | cost at 1M |
|---|--:|--:|--:|--:|--:|
| dupehound | 36 | 36 | 36 | 0.74 s | $0 |
| Claude Haiku | 20 | 16 | 6 | 276 s | $0.65 |
| Claude Opus | 22 | 13 | 0 | 820 s | $2.54 |
| Claude Sonnet | 19 | 0 | did not finish | did not finish | did not finish |

<sub>Agent figures are the mean of two runs at 1M LOC. Cost is the run's cumulative API-equivalent figure (these runs used a Max subscription, so out-of-pocket was $0); each agent run also processed several million input tokens. "Did not finish" means both Sonnet runs hit the 15-minute, 150-turn budget without returning a result. Per-run figures and run-to-run variance are in the [full writeup](benchmarks/results/2026-06-28-agents-vs-dupehound.md).</sub>

dupehound is the only system that holds its recovery flat as the repository grows, in under a second, for $0, with the same answer on every run. On renamed clones (30 of the 39 pairs) it recovers all 30 at every size, where the agents manage about half at 10,000 lines and near zero at 1,000,017, reading under 1% of files within budget and varying widely between runs (one model scored 0 of 39 and 33 of 39 on the same task). dupehound also reported zero false positives across 15,000+ real functions. Full method, per-type and per-run tables, and limitations: [benchmarks/results/2026-06-28-agents-vs-dupehound.md](benchmarks/results/2026-06-28-agents-vs-dupehound.md).

## How it works

dupehound fingerprints the structure of each function, not its text, so a copy is still matched after every identifier is renamed.

<p align="center">
  <img src="assets/pipeline.svg" alt="The pipeline: discover files, fingerprint every function via tree-sitter parsing and winnowing, match through an inverted index, report" width="900">
</p>

1. **Discover.** Walk the repo (gitignore-aware) and skip generated files.
2. **Fingerprint.** Parse every function with tree-sitter, normalize it (identifiers to `ID`, literals to `LIT`, comments dropped), and hash 10-gram windows with winnowing. A renamed copy normalizes to the same fingerprints as its original. Done per function, in parallel (MOSS, SIGMOD 2003).
3. **Match.** Compare functions by Jaccard similarity through an inverted index, cull boilerplate, and cluster with union-find. No all-pairs scan.
4. **Report.** A repo-level slop score for `scan`, a chart for `history`, and exit 1 in CI for `check`.

## Install

Prebuilt binaries for macOS, Linux and Windows are on the [releases page](https://github.com/Rafaelpta/dupehound/releases), or:

```
cargo install dupehound
```

On macOS or Linux with Homebrew:

```
brew install rafaelpta/dupehound/dupehound
```

`history` and `check` require `git` on PATH. `scan` works on any directory.

## Usage

### `scan`

`dupehound scan [path]` ranks duplicate clusters by deletable lines:

<p align="center">
  <img src="assets/scan.png" alt="dupehound scan on the vscode source tree: 2.8 percent slop, grade A, listing the top duplicate clusters" width="880">
</p>

The slop score is the percentage of code you could delete if every cluster kept only one copy; the largest copy is exempt and test files are excluded by default, since table-driven tests are repetitive by design. On Rust, trait-impl methods (`From`, `Display`, ...) are also kept out of the score, since each impl is required and cannot be merged.

<ul>
<li><code>--explain N</code> prints a cluster's code as proof</li>
<li><code>--json</code> emits a versioned schema</li>
<li><code>--card</code> writes a score card as SVG and PNG</li>
<li><code>--include-classes</code> flags C# classes with near-duplicate property and method signatures (experimental, opt-in, never affects the slop score)</li>
</ul>

Languages: TypeScript, TSX, JavaScript, Python, Rust, Go, Java, Ruby, Swift, C, C++, PHP, C#, Kotlin.

### `history`

`dupehound history` measures the slop score at monthly snapshots, reading blobs straight from the object database (no checkouts), and reports when duplication took off:

<p align="center">
  <img src="assets/history.png" alt="dupehound history charting the slop score across monthly snapshots, with the grade and the inflection point where duplication took off" width="880">
</p>

### `check`

`dupehound check` gates CI and pre-commit. It indexes the codebase at the base revision and probes only the functions a change adds or touches. <br><br> Moved functions and in-place edits don't fire. <br><br>Exit codes: 0 clean, 1 findings, 2 error.

```
$ dupehound check --diff main .
src/api/orders.ts:1 calculateOrderAmount() is a 100% duplicate of src/billing/invoice.ts:1 computeInvoiceTotal() — reuse it
      import { computeInvoiceTotal } from '../billing/invoice';
```

For Rust, Python, TypeScript, TSX and JavaScript, `check` also prints the import that replaces the duplicate. It is a suggestion only; dupehound never edits your files.

A GitHub Actions recipe and a pre-commit setup are in [docs/ci.md](docs/ci.md). <br><br>To make a coding agent reuse code instead of rewriting it, feed `check` back to it from `CLAUDE.md` or `AGENTS.md`; the snippet is there too.

### `mcp`

`dupehound mcp` runs as an MCP server over stdio, exposing `check` and `scan` as tools an AI coding agent can call itself, mid-edit, to reuse existing code instead of rebuilding it. It stays local and offline (stdio is a local pipe), deterministic, and no AI. Add it to Claude Code with:

```
claude mcp add dupehound, dupehound mcp
```

The agent then has a `check_duplication` tool (did this change duplicate existing code, and where is the original) and a `scan_duplication` tool (the repo's duplication score and clusters).

## How it works

Function bodies are parsed with tree-sitter and normalized: identifiers, strings and numbers become sentinels, comments are dropped, structure stays. k-grams of 10 tokens are rolling-hashed and selected by robust winnowing ([Schleimer, Wilkerson & Aiken, SIGMOD 2003](https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf)), which guarantees any shared run of 17 normalized tokens is caught.<br><br> An inverted fingerprint index generates candidate pairs, boilerplate fingerprints are culled, similarity is exact Jaccard, union-find builds the clusters.

The defaults are conservative about false positives: generated, minified and vendored files are skipped, functions under 40 normalized tokens are ignored, and every match is verifiable with `--explain`. <br><br>Grade buckets were calibrated against express (0.0%), gin (0.2%), tokio (1.1%), fastapi (1.7%) and vscode (2.8%), all grade A. vscode, at 3.0M lines and 54k functions, scans in 2.3s on a laptop. Full design notes in [docs/design.md](docs/design.md).

## Why dupehound

Coding agents don't know what a codebase already contains, so they re-implement it. `formatDate` becomes `renderTimestamp`, then `stringifyDate`: the same logic under several names, each copy aging independently. <br><br>GitClear's [analysis of 211 million changed lines](https://www.gitclear.com/ai_assistant_code_quality_2025_research) found duplicated code blocks grew 8x in 2024, the first year copy-pasted lines outnumbered moved ones.

An LLM doesn't do this job well.<br><br> Duplicate detection compares every function against every other; a model samples what fits in context, an index checks everything. A merge gate must be reproducible: same input, same verdict, an algorithm you can read. dupehound is the deterministic side of the loop: the agent writes, the index remembers.

## Bugs

Please file issues on [the issue tracker](https://github.com/Rafaelpta/dupehound/issues). <br><br>The most useful false-positive report is a small code pair that matches but shouldn't, plus the `--explain` output; these become regression fixtures directly.

## Contributing

PRs welcome. Adding a language is the most wanted contribution and is roughly one tree-sitter query file; see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](./LICENSE). Bundled [JetBrains Mono](https://www.jetbrains.com/lp/mono/) subsets are under the [SIL OFL 1.1](assets/fonts/OFL.txt). The diagram uses Excalidraw's [Virgil](https://github.com/excalidraw/virgil) font (OFL).
