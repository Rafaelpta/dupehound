# A hide-and-seek benchmark for code-duplication detection

**Thesis.** Deterministic structural detectors and frontier LLMs sit at opposite
ends of a cost x latency x accuracy frontier for code-clone detection. A
structural detector (dupehound) dominates Type-1/Type-2 clones at ~$0, with
millisecond latency and perfect run-to-run determinism; LLMs extend recall into
Type-3/Type-4 (gapped and semantic) clones, but at orders of magnitude more
cost and latency, with non-deterministic output and a measurable false-positive
tax. The actionable conclusion is a hybrid: a deterministic prefilter that
escalates only the residual to an LLM.

This document is the standing methodology. It is versioned with the code and is
meant to read as the skeleton of an arXiv-style report. Sections tagged
**[implemented]** describe what the harness in `harness/` already does;
sections tagged **[planned]** describe extensions on the roadmap.

---

## 1. Why another clone benchmark

Clone detection has two decades of literature and an established benchmark,
BigCloneBench (a labelled clone corpus mined from IJaDataset). What is *not*
established is the question every team now actually asks: **in the agent-loop
era, what does it cost, in dollars, seconds, and reliability, to ask an LLM
to find duplication in a real repository, and how does that compare to a
deterministic tool?** This benchmark measures exactly that, and it situates the
answer on an explicit economic frontier rather than a single accuracy number.

We deliberately keep the headline intuitive: *we hide known duplicates in real
code and measure who finds them, how fast, how cheap, and how reliably.*

## 2. The core design: planted clones (mutation-testing for duplication)

The central methodological move is borrowed from **mutation testing** and from
**spike-and-recovery** validation in analytical chemistry: rather than trying to
label all clones in a corpus after the fact (subjective, incomplete), we
**inject a known set of duplicates** and measure recovery against perfect ground
truth.

- We start from a pool of distinct, real donor functions (`harness/donors.py`).
- Each **group** is a set of copies of one donor, transformed to a target clone
 type. Every within-group pair is a true clone of that type.
- Donors are mutually distinct, so every cross-group pair is a true negative.
- Because we planted every clone, recall and precision are measured exactly.

This buys three properties the user requirements demand:

1. **Easy to read.** "We hid N duplicates; system X found Y in Z seconds for $C."
2. **Rigorous.** Ground truth is perfect; precision is not estimated, it is counted.
3. **Honest by construction.** Two honesty mechanisms are built in:
  - The corpus includes the cases where the structural detector is *expected
   to lose* (Type-3 with large gaps, Type-4 semantic), so the report shows
   where LLMs add value, not only where dupehound wins.
  - Precision is reported beside recall: a detector that flags everything
   "recovers" all planted clones and is still useless. A predicted pair that
   is not a planted pair is a genuine false positive (the corpus contains no
   unplanted duplication; empirically, dupehound's precision of 1.000 is the
   self-check that the donor pool is mutually distinct at the test threshold).

## 3. Clone taxonomy and generation **[implemented for Type-1..3, partial Type-4]**

| Type | Name | Generation |
|---|---|---|
| 1 | exact | identical copy; whitespace/comments may vary |
| 2 | renamed | every identifier and literal systematically renamed; structure intact |
| 3 | gapped | a Type-2 copy with k injected statements (gap size swept: 1..4) |
| 4 | semantic | hand-authored variant pairs: same contract, different implementation |

Type-1/2/3 are generated mechanically from donor templates, so they scale and
are language-portable. **Type-4 is not auto-generated**, reliable synthesis of
semantically-equivalent-but-syntactically-different code is itself an open
problem. The synthetic tier ships a small hand-authored Type-4 set; broad Type-4
coverage comes from the mined tier (Section 4.2).

## 4. Datasets

### 4.1 Synthetic mutation corpus **[implemented]**

`harness/plant.py --seed S` deterministically builds a corpus and
`ground_truth.json`. The default profile plants 32 functions across 15 groups (19
clone pairs spanning Type-1..4) and drives the CI tier (Section 9).

A second **`realistic` profile** is used for the large-scale agent comparison: it
weights the corpus to how duplication actually occurs, ~77% Type-2 (copy-paste
then rename, including the function name, so there is no grep-by-name shortcut),
with a little Type-1/Type-3 and a few Type-4 kept for honesty (~39 pairs / 42
functions). This is fidelity to real duplication, not tuning for a result: real
"duplication you would delete" is overwhelmingly renamed copy-paste, while
semantic Type-4 is rare. Each needle is also embedded among **sub-threshold
filler functions** (each below `min-tokens`, so the detector ignores them and
they create no accidental clones) so a planted function sits inside an ordinary-
looking small module rather than a tell-tale one-function file. Detected/reported
functions are matched to ground truth by `(file basename, function name)`.

### 4.2 Refactoring-reversal mined corpus **[planned]**

Mine real git history for commits that *removed* duplication ("extract
function", de-dup PRs) and **reverse** them to reconstruct the duplicated state.
The ground truth is then a real engineer's real de-duplication decision, not an
annotator's opinion, realistic, automatically labelled, and (using recent
commits) free of training-data leakage. This is the SZZ-style change-mining idea
applied to duplication, and it is where realistic Type-3/4 coverage comes from.

### 4.3 Real-repo hosting + size slicing **[implemented for TypeScript]**

`harness/host.py` plants the needle set into a large real TypeScript repo
(default `microsoft/vscode` at a pinned commit), sliced into nested corpora of
growing size (10k / 100k / 500k host LOC). The clone is disk-safe: a treeless +
cone-sparse partial checkout materializes only the source dirs we ask for (tens
of MB). The same needles go into every slice, so any change across slices is
attributable to repo size alone, the scaling curve.

**Baseline subtraction comes for free.** Only needle-vs-needle pairs map to
ground truth (matched by file basename); pairs among host functions never map
and are dropped. So the host's own duplication cannot inflate false positives,
and the controlled metric stays on needle recovery. The host's role is to be the
haystack that makes discovery realistically hard and expensive at scale.
Empirically, dupehound holds precision 1.000 while scanning 15k+ real VS Code
functions, it flags none of them as a planted clone. Donor templates currently
cover Python and TypeScript; the other 12 dupehound languages are a follow-up.

## 5. Systems under test

- **dupehound** at a pinned version/commit (`harness/systems/dupehound.py`).
 Deterministic; tokens and cost are zero by construction.
- **Autonomous agent panel** (`harness/systems/agent.py`) **[implemented]**: a
 headless `claude -p` agent given read-only tools (Read/Glob/Grep), the slice
 directory as its working tree, and the task of finding duplicate functions on
 its own, the realistic agent-loop framing. Model tiers Haiku / Sonnet / Opus
 span cheap to frontier (the Ponytail choice). Cost, token counts (including
 cache-read/creation), and turn count come from the CLI JSON result; wall-clock
 is measured by the harness. Cross-provider agents (GPT/Gemini/open) are a
 follow-up.
- **Single-prompt LLM panel** (`harness/systems/llm.py`): the whole function set
 in one prompt, only viable for the small synthetic corpus. Ships a `mock`
 backend labelled **(sim)** that reproduces the JSON contract to validate the
 pipeline without spend; not a model result.

**[planned]** A classical academic baseline (e.g. SourcererCC) can be added to
situate dupehound against prior structural detectors for a formal submission.

## 6. Experiments

- **Repo-scale discovery [implemented].** Each system finds clone pairs across a
 whole repo. The **chunking confound is resolved by handing the LLM a real
 autonomous agent with tools** (Section 5): the agent decides what to read and
 grep, so "how the LLM copes with a repo too big for its context" becomes part
 of the measured system rather than a harness assumption. This is the setting
 where agent cost, latency, and budget-truncation are exposed.
- **Scaling curve [implemented].** The same needles are planted into slices of
 growing size (Section 4.3). Recovery, latency, and cost are reported per size,
 exposing the divergence: dupehound flat (sub-second, $0, deterministic) vs.
 agents whose cost and latency climb and which can hit their budget ceiling on
 the largest slice.
- **Pairwise discrimination [planned].** "Given two fragments, are they clones?"
 isolates detection capability from retrieval, per clone type, the protocol
 used to evaluate classical detectors on BigCloneBench.

## 7. Metrics **[implemented]**

The evaluation unit is the unordered function pair. The universe is all C(n,2)
candidate pairs; planted pairs are positives, the rest negatives.

- **Precision, recall, F1, MCC** from the pair-level confusion matrix.
- **Recall by clone type** (the honesty curve).
- **Cost**: USD/run and **USD per true positive**; tokens in/out.
- **Latency**: wall-clock per run.
- **Determinism**: mean pairwise Jaccard of the predicted-pair set across n
 runs (1.000 = identical every run). dupehound is run n times too, so its
 determinism is measured, not assumed.

## 8. Statistical treatment

- n >= 5 runs per system; report mean +/- sample standard deviation.
- **[planned]** Bootstrap confidence intervals on F1; **McNemar's test** for
 paired comparison of two systems on the same candidate pairs.
- LLM snapshots and run dates are recorded; model endpoints drift, so every
 result file is dated and pinned.

## 9. Reproducibility protocol **[implemented, two tiers]**

- **Deterministic tier (CI, every PR/tag, free).** dupehound over the synthetic
 corpus; recomputes P/R/F1 and fails on regression in dupehound's own accuracy.
 No network, no spend. This is the Ponytail-style "runs on every feature" loop.
- **Full tier (per release / manual, paid).** Adds the LLM panel. Results are
 written to `results/<date>-<tag>.{md,json}`, dated and versioned, because
 model endpoints and prices change. Provenance (seed, corpus stats, dupehound
 version, threshold, token-counting method) is recorded in every file.

Pinning: a citable run must use a dupehound binary built from a recorded commit,
fixed `--seed`, fixed `--threshold`/`--min-tokens`, pinned model snapshot ids,
and provider-published prices entered in `systems/llm.py`.

## 10. Threats to validity

1. **Training-data leakage.** LLMs may have seen public corpora; this inflates
  their recall. Mitigated by the synthetic tier (generated, novel) and the
  planned mined tier (recent commits). Stated, not hidden.
2. **Synthetic distribution.** Mechanically planted clones need not match the
  distribution of natural ones; the mined and real-repo tiers address external
  validity.
3. **Type-4 generation is limited.** Synthetic Type-4 is a small hand-authored
  set; do not over-read synthetic Type-4 numbers.
4. **Agent strategy + budget truncation (repo scale).** The agent decides what
  to read/grep, so results reflect that agent harness, not the model in the
  abstract; a different scaffold would shift the numbers. Per-run spend is
  capped (`--max-budget-usd`), so on the largest slices an agent may stop early
  and return partial results, reported honestly as a budget-truncated point,
  not a model failure.
5. **Prompt sensitivity.** Report results across >= 2 prompts; pre-register the
  primary prompt.
6. **Ground-truth incompleteness on real repos.** When planting into non-empty
  hosts, unplanted-but-genuine duplication must be baseline-subtracted to avoid
  counting true finds as false positives.
7. **Threshold dependence.** dupehound's Type-3 recall is a function of the
  similarity threshold; a threshold sweep is reported rather than a single point.
8. **Mock disclosure.** Simulated rows are labelled and never presented as model
  results.
9. **Agent-benchmark hygiene (learned the hard way).** Three contamination modes
  were found and fixed by auditing what the agent actually did, not by trusting
  its score. (a) **MCP leakage:** a headless agent inherits the environment's
  MCP servers, including dupehound itself, and will simply call the tool
  under test; isolate with `--strict-mcp-config` + `--permission-mode dontAsk` +
  disallowing sub-agent tools. (b) **Signpost folder:** needles grouped in one
  obviously-named directory let the agent read that folder and skip the
  haystack, erasing the scaling effect; needles must be scattered through the
  host tree under plausible, unique names. (c) **Answer key in the corpus:** a
  ground-truth file inside the scanned directory can be read by the agent; it
  must live outside the corpus. dupehound is immune to all three (it scans
  structurally and never calls itself), but any agent arm must be audited at the
  tool-call level, not the score level.

## 11. Roadmap to a full report

1. Synthetic mutation tier + repo-scale discovery + economic frontier. **[done]**
2. TypeScript donors + large real-repo hosting + size slices. **[done]**
3. Autonomous-agent arm (Haiku/Sonnet/Opus) + scaling curve over repo size. **[done]**
4. Refactoring-reversal mined corpus (Section 4.2).
5. Remaining languages; pairwise-discrimination experiment; threshold sweep.
6. Bootstrap CIs + McNemar; the Pareto-frontier figure (recovery vs $, vs latency).
7. Optional classical baseline (SourcererCC) for formal submission.
