<p align="center">
  <img src="assets/hound.png" alt="dupehound" width="200">
</p>

<h1 align="center">dupehound</h1>

<p align="center">Finds functions duplicated by AI, even after every identifier is renamed.</p>

<p align="center">
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

Everything runs locally and deterministically: no network, no API keys, no machine learning.

<p align="center">
  <img src="assets/pipeline.svg" alt="The pipeline: discover files, fingerprint every function via tree-sitter parsing and winnowing, match through an inverted index, report" width="900">
</p>

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

`dupehound scan [path]` ranks duplicate clusters by deletable lines:

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

The slop score is the percentage of code you could delete if every cluster kept only one copy; the largest copy is exempt and test files are excluded by default, since table-driven tests are repetitive by design. `--explain N` prints a cluster's code as proof, `--json` emits a versioned schema, `--card` writes a score card as SVG and PNG. Languages: TypeScript, TSX, JavaScript, Python, Rust, Go, Java, Ruby, Swift, C, C++, PHP.

`dupehound history` measures the slop score at monthly snapshots, reading blobs straight from the object database (no checkouts), and reports when duplication took off:

```
   36.1% ┤                      ██
         ┤                  ▂▂▆▆██
         ┤              ▂▂████████
         ┤          ▁▁████████████
    0.0% ┤          ██████████████
         └────────────────────────
          2025-01          2025-12

  current slop score: 36.1% (grade F)
  duplication went from ~0 to 36.1% since 2025-05
```

`dupehound check` gates CI and pre-commit. It indexes the codebase at the base revision and probes only the functions a change adds or touches. Moved functions and in-place edits don't fire. Exit codes: 0 clean, 1 findings, 2 error.

```
$ dupehound check --diff main .
src/api/orders.ts:1 calculateOrderAmount() is a 100% duplicate of src/billing/invoice.ts:1 computeInvoiceTotal() — reuse it
```

A GitHub Actions recipe and a pre-commit setup are in [docs/ci.md](docs/ci.md). To make a coding agent reuse code instead of rewriting it, feed `check` back to it from `CLAUDE.md` or `AGENTS.md`; the snippet is there too.

## How it works

Function bodies are parsed with tree-sitter and normalized: identifiers, strings and numbers become sentinels, comments are dropped, structure stays. k-grams of 10 tokens are rolling-hashed and selected by robust winnowing ([Schleimer, Wilkerson & Aiken, SIGMOD 2003](https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf)), which guarantees any shared run of 17 normalized tokens is caught. An inverted fingerprint index generates candidate pairs, boilerplate fingerprints are culled, similarity is exact Jaccard, union-find builds the clusters.

The defaults are conservative about false positives: generated, minified and vendored files are skipped, functions under 40 normalized tokens are ignored, and every match is verifiable with `--explain`. Grade buckets were calibrated against express (0.0%), gin (0.2%), tokio (1.1%), fastapi (1.7%) and vscode (2.8%), all grade A. vscode, at 2.97M lines and 53k functions, scans in 3.6s on a laptop. Full design notes in [docs/design.md](docs/design.md).

## Why dupehound

Coding agents don't know what a codebase already contains, so they re-implement it. `formatDate` becomes `renderTimestamp`, then `stringifyDate`: the same logic under several names, each copy aging independently. GitClear's [analysis of 211 million changed lines](https://www.gitclear.com/ai_assistant_code_quality_2025_research) found duplicated code blocks grew 8x in 2024, the first year copy-pasted lines outnumbered moved ones.

An LLM can't do this job. Duplicate detection compares every function against every other; a model samples what fits in context, an index checks everything. A merge gate must be reproducible: same input, same verdict, an algorithm you can read. dupehound is the deterministic side of the loop: the agent writes, the index remembers.

## Bugs

Please file issues on [the issue tracker](https://github.com/Rafaelpta/dupehound/issues). The most useful false-positive report is a small code pair that matches but shouldn't, plus the `--explain` output; these become regression fixtures directly.

## Contributing

PRs welcome. Adding a language is the most wanted contribution and is roughly one tree-sitter query file; see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](./LICENSE). Bundled [JetBrains Mono](https://www.jetbrains.com/lp/mono/) subsets are under the [SIL OFL 1.1](assets/fonts/OFL.txt). The diagram uses Excalidraw's [Virgil](https://github.com/excalidraw/virgil) font (OFL).
