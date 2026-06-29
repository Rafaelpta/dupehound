#!/usr/bin/env python3
"""Large real-codebase host ingestion and size slicing.

Builds the "haystack": a real, large TypeScript repository, sliced into nested
corpora of growing size, each seeded with the same set of planted needles. The
needle set is identical across slices, so any change in a system's recovery,
time, or cost across slices is attributable to repo size alone -- the scaling
curve.

Disk-safe: uses a treeless + cone-sparse partial clone so only the source dirs
we ask for are ever materialized (tens of MB), never the full repo history.

Scoring note: only needle-vs-needle pairs map to ground truth; pairs among host
functions never map and are dropped, so the controlled metric measures needle
recovery and the host's own duplication does not pollute it. The host's role is
to make discovery realistically hard and expensive at scale.

Usage:
    python3 host.py --repo microsoft/vscode --targets 10000 100000 500000
"""
import argparse
import json
import os
import random
import shutil
import subprocess

import paths
import plant

# Plausible TS module names so planted needles blend into the host tree instead
# of standing out. Needles are scattered across real host directories with these
# names (made unique), so a system must actually search the codebase to find
# them -- there is no signpost folder to shortcut to.
NEEDLE_NAMES = [
    "formatter", "tokenizer", "parser", "resolver", "scheduler", "registry",
    "validator", "serializer", "collector", "dispatcher", "aggregator",
    "normalizer", "comparator", "iterator", "builder", "matcher", "encoder",
    "walker", "differ", "ranker", "splitter", "merger", "indexer", "sanitizer",
    "tracker", "planner", "emitter", "selector", "reducer", "scanner",
    "accumulator", "inspector", "wrapper", "provider", "adapter", "observer",
]

HOSTS_DIR = os.path.join(paths.data_dir(), "hosts")
SLICES_DIR = os.path.join(paths.data_dir(), "slices")

# Source dirs materialized from the host. base+editor+platform+workbench is well
# over 1M LOC of non-test TypeScript (~150 MB, still disk-safe via treeless+sparse).
DEFAULT_CONE = ["src/vs/base", "src/vs/editor", "src/vs/platform", "src/vs/workbench"]


def _run(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError("cmd failed: %s\n%s" % (" ".join(cmd), p.stderr[:800]))
    return p.stdout.strip()


def ensure_host(repo, cone=DEFAULT_CONE, commit=None):
    """Treeless + cone-sparse clone; materialize only `cone` dirs. Idempotent."""
    dest = os.path.abspath(os.path.join(HOSTS_DIR, repo.replace("/", "__")))
    url = "https://github.com/%s.git" % repo
    if not os.path.isdir(os.path.join(dest, ".git")):
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        _run(["git", "clone", "--depth", "1", "--filter=blob:none",
              "--no-checkout", url, dest])
        _run(["git", "sparse-checkout", "init", "--cone"], cwd=dest)
    if commit:
        _run(["git", "fetch", "--depth", "1", "origin", commit], cwd=dest)
        _run(["git", "checkout", commit], cwd=dest)
    _run(["git", "sparse-checkout", "set"] + cone, cwd=dest)
    _run(["git", "checkout"], cwd=dest)
    sha = _run(["git", "rev-parse", "HEAD"], cwd=dest)
    return dest, sha


def collect_ts_files(host_dir):
    """Sorted (rel_path, loc) for non-test .ts files -- deterministic ordering."""
    out = []
    for root, _dirs, files in os.walk(host_dir):
        if ".git" in root:
            continue
        for fn in files:
            if not fn.endswith(".ts") or fn.endswith(".d.ts") or fn.endswith(".test.ts"):
                continue
            if os.sep + "test" + os.sep in (root + os.sep):
                continue
            full = os.path.join(root, fn)
            try:
                with open(full, encoding="utf-8") as f:
                    loc = sum(1 for _ in f)
            except (UnicodeDecodeError, OSError):
                continue
            out.append((os.path.relpath(full, host_dir), loc))
    out.sort()
    return out


def pick_slice(files, target_loc):
    """Prefix of the file list whose cumulative LOC first reaches target."""
    chosen, total = [], 0
    for rel, loc in files:
        chosen.append((rel, loc))
        total += loc
        if total >= target_loc:
            break
    return chosen, total


def make_slice(host_dir, slice_files, needles_dir, gt, slice_dir, rng):
    """Assemble a slice corpus: host files + needles scattered through the tree.

    Needles are written into real host directories under plausible, unique names
    (no signpost folder), so a system must search the codebase to find them. The
    ground truth is NOT written inside the scanned dir -- the caller stores it
    elsewhere so no agent can read the answer key.
    """
    if os.path.isdir(slice_dir):
        shutil.rmtree(slice_dir)
    os.makedirs(slice_dir)
    host_dirs, used_basenames = set(), set()
    for rel, _loc in slice_files:
        dst = os.path.join(slice_dir, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(os.path.join(host_dir, rel), dst)
        if os.path.dirname(rel):
            host_dirs.add(os.path.dirname(rel))
        used_basenames.add(os.path.basename(rel))
    dirs = sorted(host_dirs) or [""]

    names = NEEDLE_NAMES[:]
    rng.shuffle(names)
    new_funcs = []
    for i, fn in enumerate(gt["functions"]):
        base, k = names[i % len(names)], 0
        cand = "%s.ts" % base
        while cand in used_basenames:
            k += 1
            cand = "%s%d.ts" % (base, k)
        used_basenames.add(cand)
        rel = os.path.join(rng.choice(dirs), cand)
        dst = os.path.join(slice_dir, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(os.path.join(needles_dir, fn["file"]), dst)
        nf = dict(fn)
        nf["file"] = rel
        new_funcs.append(nf)
    slice_gt = dict(gt)
    slice_gt["functions"] = new_funcs
    return slice_gt


def build_slices(repo, targets, seed=7, lang="ts", commit=None, profile="default"):
    host_dir, sha = ensure_host(repo, commit=commit)
    files = collect_ts_files(host_dir)
    total_loc = sum(loc for _r, loc in files)

    # generate the needle set once
    needles_dir = os.path.abspath(os.path.join(SLICES_DIR, "_needles-%s" % lang))
    gt = plant.build(needles_dir, seed, lang, profile)

    # ground truth lives OUTSIDE the scanned slice dirs so no agent can read it
    meta_dir = os.path.join(SLICES_DIR, "_meta")
    os.makedirs(meta_dir, exist_ok=True)

    slices = []
    for idx, target in enumerate(sorted(targets)):
        chosen, loc = pick_slice(files, target)
        name = "%s-%s" % (repo.replace("/", "_"), _label(target))
        slice_dir = os.path.abspath(os.path.join(SLICES_DIR, name))
        rng = random.Random(seed * 1000 + idx)
        slice_gt = make_slice(host_dir, chosen, needles_dir, gt, slice_dir, rng)
        gt_path = os.path.abspath(os.path.join(meta_dir, "%s.gt.json" % name))
        with open(gt_path, "w") as f:
            json.dump(slice_gt, f, indent=2)
        slices.append({
            "name": name, "dir": slice_dir, "gt_path": gt_path,
            "target_loc": target, "host_loc": loc, "host_files": len(chosen),
            "needle_functions": len(gt["functions"]),
            "needle_pairs": len(gt["pairs"]),
        })
    meta = {"repo": repo, "commit": sha, "lang": lang, "seed": seed,
            "available_loc": total_loc, "cone": DEFAULT_CONE, "slices": slices}
    with open(os.path.join(SLICES_DIR, "slices.json"), "w") as f:
        json.dump(meta, f, indent=2)
    return meta


def _label(loc):
    if loc >= 1_000_000:
        return "%dm" % (loc // 1_000_000)
    if loc >= 1000:
        return "%dk" % (loc // 1000)
    return str(loc)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="microsoft/vscode")
    ap.add_argument("--commit", default=None)
    ap.add_argument("--targets", type=int, nargs="+",
                    default=[10000, 100000, 500000, 1000000])
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--lang", default="ts")
    ap.add_argument("--profile", default="default")
    args = ap.parse_args()
    os.makedirs(SLICES_DIR, exist_ok=True)
    meta = build_slices(args.repo, args.targets, args.seed, args.lang,
                        args.commit, args.profile)
    print("host: %s @ %s  (%d LOC available)"
          % (meta["repo"], meta["commit"][:10], meta["available_loc"]))
    for s in meta["slices"]:
        print("  slice %-28s host %7d LOC in %4d files + %d needles"
              % (s["name"], s["host_loc"], s["host_files"], s["needle_functions"]))


if __name__ == "__main__":
    main()
