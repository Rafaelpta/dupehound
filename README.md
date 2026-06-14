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

### `scan`

`dupehound scan [path]` ranks duplicate clusters by deletable lines:

```
$ dupehound scan .

  dupehound v0.1.0 — scanned 10,398 files · 2,995,077 lines · 53,984 functions in 2.6s

  ╭──────────────────────────────────────────────────────────────────╮
  │  SLOP SCORE   2.8%   grade A                                     │
  │  65,804 of 2,319,912 significant lines are deletable duplicates  │
  ╰──────────────────────────────────────────────────────────────────╯

  3556 duplicate clusters · showing top 10 by deletable lines  (--all for everything)

  ● Cluster 1 ─ 2 copies · 100% similar · 1,265 deletable lines · [tests, not scored]
    ★ …ension/prompts/node/test/fixtures/extHost.api.impl.ts:116  createApiFactoryAndRegister…  1265 lines
      …t/test/simulation/fixtures/vscode/extHost.api.impl.ts:116  createApiFactoryAndRegister…  1265 lines   100% █████████

  ● Cluster 2 ─ 2 copies · 82% similar · 696 deletable lines ─────────────────
    ★ …n/chatSessions/vscode-node/copilotCLIChatSessions.ts:1046  registerCLIChatCommands  719 lines
      …ns/vscode-node/copilotCLIChatSessionsContribution.ts:2177  registerCLIChatCommands  696 lines    82% ███████▍

  ● Cluster 3 ─ 6 copies · 81% similar · 457 deletable lines ─────────────────
    ★ …ot/src/extension/prompts/node/agent/geminiPrompts.tsx:136  render  103 lines
      …nsion/prompts/node/agent/defaultAgentInstructions.tsx:113  render   95 lines    87% ███████▉
      …lot/src/extension/prompts/node/agent/geminiPrompts.tsx:33  render   92 lines    82% ███████▍
      …opilot/src/extension/prompts/node/agent/xAIPrompts.tsx:19  render   92 lines    73% ██████▌
      …sion/prompts/node/agent/openai/defaultOpenAIPrompt.tsx:28  render   90 lines    83% ███████▌
      …/src/extension/prompts/node/agent/anthropicPrompts.tsx:84  render   88 lines    81% ███████▎

  ● Cluster 4 ─ 4 copies · 98% similar · 392 deletable lines · [tests, not scored]
    ★ …/src/extension/prompts/node/test/fixtures/EditForm.tsx:12  EditForm  134 lines
      …ot/src/platform/parser/test/node/fixtures/EditForm.tsx:12  EditForm  134 lines   100% █████████
      …/test/simulation/fixtures/edit/issue-7487/EditForm.tsx:12  EditForm  134 lines   100% █████████
      …ion/prompts/node/test/fixtures/EditForm.summarized.tsx:12  EditForm  124 lines    94% ████████▍

  ● Cluster 5 ─ 3 copies · 82% similar · 335 deletable lines ─────────────────
    ★ …/src/extension/prompts/node/agent/vscModelPrompts.tsx:453  render  216 lines
      …/src/extension/prompts/node/agent/vscModelPrompts.tsx:274  render  176 lines    80% ███████▎
      …t/src/extension/prompts/node/agent/vscModelPrompts.tsx:17  render  159 lines    83% ███████▌

  ● Cluster 6 ─ 2 copies · 87% similar · 323 deletable lines ─────────────────
    ★ …h/contrib/chat/common/chatService/chatServiceImpl.ts:1080  _sendRequestAsync    377 lines
      …h/contrib/chat/common/chatService/chatServiceImpl.ts:1112  sendRequestInternal  323 lines    87% ███████▉

  ● Cluster 7 ─ 2 copies · 100% similar · 309 deletable lines ────────────────
    ★ …ilot/src/util/vs/editor/common/core/edits/textEdit.ts:219  compose  309 lines
      src/vs/editor/common/core/edits/textEdit.ts:217             compose  309 lines   100% █████████

  ● Cluster 8 ─ 2 copies · 100% similar · 292 deletable lines ────────────────
    ★ …ot/src/extension/prompts/node/agent/familyHPrompts.tsx:17  render  292 lines
      …ot/src/extension/prompts/node/agent/minimaxPrompts.tsx:18  render  292 lines   100% █████████

  ● Cluster 9 ─ 2 copies · 85% similar · 282 deletable lines ─────────────────
    ★ …ot/src/extension/byok/vscode-node/anthropicProvider.ts:95  provideLanguageModelChatRes…  332 lines
      …t/src/extension/byok/vscode-node/anthropicProvider.ts:108  doRequest                     282 lines    85% ███████▋

  ● Cluster 10 ─ 7 copies · 100% similar · 276 deletable lines ───────────────
    ★ …xtension/prompts/node/test/fixtures/map.summarized.ts:236  touch   46 lines
      …pilot/src/extension/prompts/node/test/fixtures/map.ts:505  touch   46 lines   100% █████████
      …on/typescriptContext/serverPlugin/src/common/utils.ts:338  touch   46 lines   100% █████████
      extensions/copilot/src/util/vs/base/common/map.ts:548       touch   46 lines   100% █████████
      …tures/tests/generate-for-selection/base/common/map.ts:505  touch   46 lines   100% █████████
      extensions/git/src/cache.ts:341                             touch   46 lines   100% █████████
      src/vs/base/common/map.ts:549                               touch   46 lines   100% █████████

  … 3546 more clusters (71,417 deletable lines) — run with --all

  skipped files: 640 generated · 40 minified
  slop score = lines deletable if every duplicate cluster kept one copy
  ★ = representative (kept) · dupehound scan --explain 1 shows the code
```

The slop score is the percentage of code you could delete if every cluster kept only one copy; the largest copy is exempt and test files are excluded by default, since table-driven tests are repetitive by design. `--explain N` prints a cluster's code as proof, `--json` emits a versioned schema, `--card` writes a score card as SVG and PNG. Languages: TypeScript, TSX, JavaScript, Python, Rust, Go, Java, Ruby, Swift, C, C++, PHP.

### `history`

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

### `check`

`dupehound check` gates CI and pre-commit. It indexes the codebase at the base revision and probes only the functions a change adds or touches. Moved functions and in-place edits don't fire. Exit codes: 0 clean, 1 findings, 2 error.

```
$ dupehound check --diff main .
src/api/orders.ts:1 calculateOrderAmount() is a 100% duplicate of src/billing/invoice.ts:1 computeInvoiceTotal() — reuse it
```

A GitHub Actions recipe and a pre-commit setup are in [docs/ci.md](docs/ci.md). To make a coding agent reuse code instead of rewriting it, feed `check` back to it from `CLAUDE.md` or `AGENTS.md`; the snippet is there too.

## How it works

Function bodies are parsed with tree-sitter and normalized: identifiers, strings and numbers become sentinels, comments are dropped, structure stays. k-grams of 10 tokens are rolling-hashed and selected by robust winnowing ([Schleimer, Wilkerson & Aiken, SIGMOD 2003](https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf)), which guarantees any shared run of 17 normalized tokens is caught. An inverted fingerprint index generates candidate pairs, boilerplate fingerprints are culled, similarity is exact Jaccard, union-find builds the clusters.

The defaults are conservative about false positives: generated, minified and vendored files are skipped, functions under 40 normalized tokens are ignored, and every match is verifiable with `--explain`. Grade buckets were calibrated against express (0.0%), gin (0.2%), tokio (1.1%), fastapi (1.7%) and vscode (2.8%), all grade A. vscode, at 3.0M lines and 54k functions, scans in 2.6s on a laptop. Full design notes in [docs/design.md](docs/design.md).

## Why dupehound

Coding agents don't know what a codebase already contains, so they re-implement it. `formatDate` becomes `renderTimestamp`, then `stringifyDate`: the same logic under several names, each copy aging independently. GitClear's [analysis of 211 million changed lines](https://www.gitclear.com/ai_assistant_code_quality_2025_research) found duplicated code blocks grew 8x in 2024, the first year copy-pasted lines outnumbered moved ones.

An LLM can't do this job. Duplicate detection compares every function against every other; a model samples what fits in context, an index checks everything. A merge gate must be reproducible: same input, same verdict, an algorithm you can read. dupehound is the deterministic side of the loop: the agent writes, the index remembers.

## Bugs

Please file issues on [the issue tracker](https://github.com/Rafaelpta/dupehound/issues). The most useful false-positive report is a small code pair that matches but shouldn't, plus the `--explain` output; these become regression fixtures directly.

## Contributing

PRs welcome. Adding a language is the most wanted contribution and is roughly one tree-sitter query file; see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](./LICENSE). Bundled [JetBrains Mono](https://www.jetbrains.com/lp/mono/) subsets are under the [SIL OFL 1.1](assets/fonts/OFL.txt). The diagram uses Excalidraw's [Virgil](https://github.com/excalidraw/virgil) font (OFL).
