#!/usr/bin/env python3
"""Planted-clone corpus generator (the "hide-and-seek" dataset).

Plants a known set of duplicate functions into a fresh source tree and records
exact ground truth. Every group is a set of copies derived from one donor; all
within-group pairs are true clones of the group's clone type. Pairs drawn from
different groups are the negatives. Because every clone is planted by us, recall
and precision are measured against perfect ground truth.

Deterministic: same --seed always yields the same corpus and ground truth.

Usage:
    python3 plant.py --seed 7 --out ../datasets/synthetic-py-v1
"""
import argparse
import importlib
import itertools
import json
import os
import random
import shutil

import paths


def _name_after(src, kw):
    first = src.splitlines()[0]
    return first[len(kw):first.index("(")].strip()


def _ts_name(src):
    first = src.splitlines()[0]
    marker = "function "
    if marker in first:
        after = first.split(marker, 1)[1]
        return after[:after.index("(")].strip()
    return first.split("(")[0].strip()


# Per-language adapters. The group/clone-type/rename/gap logic is language
# agnostic; only the file extension, the Type-3 gap statement, and the function-
# name parser differ. Donor templates live in a per-language module.
LANGS = {
    "python": {
        "ext": "py",
        "module": "donors",
        "gap": lambda i: "    audit_%d = %d" % (i, i * 7 + 3),
        "func_name": lambda s: _name_after(s, "def "),
    },
    "ts": {
        "ext": "ts",
        "module": "donors_ts",
        "gap": lambda i: "  const audit%d = %d;" % (i, i * 7 + 3),
        "func_name": _ts_name,
    },
}

# Realistic identifier pools for Type-2/3 systematic renaming. Drawn without
# replacement per copy so a copy never collides with itself.
NAME_POOL = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
    "data", "buffer", "cursor", "token", "payload", "entry", "node", "slot",
    "acc", "tmp", "ptr", "head", "tail", "count", "total", "value", "items",
    "rows", "cols", "keys", "vals", "out", "res", "agg", "store", "cache",
    "left", "right", "lo", "hi", "mid", "span", "delta_t", "win", "step",
]
# VS Code-style camelCase domain names so a planted (renamed) function does not
# stand out from real code -- an agent cannot spot needles by their odd names.
# Large enough that names do not repeat across the planted set.
FN_POOL = [
    "resolveReference", "computeRange", "formatLabel", "parseToken", "buildIndex",
    "mergeRanges", "scanTokens", "foldValues", "collectStats", "applyRules",
    "normalizeInput", "serializeNode", "dispatchEvent", "registerHandler",
    "validateEntry", "transformRow", "evaluateExpression", "summarizeData",
    "rankResults", "filterNoise", "resolveScope", "computeOffset", "trackUsage",
    "encodeValue", "decodeValue", "walkTree", "flattenNodes", "groupEntries",
    "splitSegments", "joinSegments", "renderLabel", "selectCandidates",
    "reduceItems", "scheduleTask", "cancelTask", "lookupSymbol", "bindContext",
    "createSession", "disposeSession", "compareNodes", "matchPattern",
    "sanitizeText", "inspectState", "annotateRange", "deriveKey", "hashContent",
    "indexFunctions", "collectDiagnostics", "resolveModule", "emitSignal",
]

# Default corpus profile: (donor_name, clone_type, copies). Each donor is used
# by exactly one group, so no two groups are accidental clones of each other.
# clone_type 3 entries carry a gap size; small gaps stay detectable, large gaps
# are expected to slip past a structural detector -- an honest recall curve.
PROFILE = [
    ("invoice_total",     1, 3, 0),
    ("merge_sorted",      1, 2, 0),
    ("moving_average",    2, 3, 0),
    ("validate_user",     2, 2, 0),
    ("parse_query",       2, 2, 0),
    ("group_by_key",      3, 2, 1),   # Type-3, small gap
    ("normalize_scores",  3, 2, 1),   # Type-3, small gap
    ("histogram",         3, 2, 2),   # Type-3, medium gap
    ("retry_call",        3, 2, 3),   # Type-3, large gap
    ("balance_brackets",  3, 2, 4),   # Type-3, large gap
    ("rate_limiter",      1, 2, 0),
    ("flatten_tree",      2, 2, 0),
]

# Realism-weighted profile: real duplication is overwhelmingly copy-paste-then-
# rename (Type-2, every identifier AND the function name changed -- no grep-by-
# name shortcut). ~77% Type-2, a little Type-1/Type-3, and a few Type-4 kept for
# honesty (where agents can still win). ~39 clone pairs across 42 functions.
PROFILE_REALISTIC = [
    ("invoice_total",     2, 3, 0),
    ("moving_average",    2, 3, 0),
    ("validate_user",     2, 3, 0),
    ("parse_query",       2, 3, 0),
    ("group_by_key",      2, 3, 0),
    ("normalize_scores",  2, 3, 0),
    ("histogram",         2, 3, 0),
    ("rate_limiter",      2, 3, 0),
    ("flatten_tree",      2, 3, 0),
    ("balance_brackets",  2, 3, 0),
    ("merge_sorted",      1, 3, 0),   # Type-1 (exact copy-paste)
    ("retry_call",        3, 3, 2),   # Type-3 (gapped)
]

PROFILES = {"default": PROFILE, "realistic": PROFILE_REALISTIC}


def render_type1(donor):
    return donor["template"].format(**donor["idents"], **donor.get("lits", {}))


def fresh_idents(donor, rng):
    """Assign a fresh, collision-free name to every identifier slot."""
    names = NAME_POOL[:]
    rng.shuffle(names)
    fns = FN_POOL[:]
    rng.shuffle(fns)
    mapping = {}
    for slot in donor["idents"]:
        if slot == "fn":
            mapping[slot] = fns.pop()
        else:
            mapping[slot] = names.pop()
    return mapping


def fresh_lits(donor, rng):
    """Mutate literals to fresh, self-consistent values (Type-2 allows this)."""
    out = {}
    for slot, val in donor.get("lits", {}).items():
        v = val.strip()
        if v.startswith('"') or v.startswith("'"):
            out[slot] = '"k%d"' % rng.randint(1000, 9999)
        else:
            try:
                int(v)
                out[slot] = str(rng.choice([0, 1, 2, 3, 8, 16, 100]))
            except ValueError:
                out[slot] = val
    return out


def render_type2(donor, rng):
    idents = fresh_idents(donor, rng)
    lits = fresh_lits(donor, rng)
    return donor["template"].format(**idents, **lits)


def inject_gap(source, gap, langcfg):
    """Insert `gap` standalone statements right after the signature line (Type-3)."""
    lines = source.splitlines()
    inserts = [langcfg["gap"](i) for i in range(gap)]
    out = [lines[0]] + inserts + lines[1:]
    return "\n".join(out) + "\n"


def render(donor, clone_type, gap, rng, langcfg):
    if clone_type == 1:
        return render_type1(donor)
    if clone_type == 2:
        return render_type2(donor, rng)
    if clone_type == 3:
        return inject_gap(render_type2(donor, rng), gap, langcfg)
    raise ValueError("use render_semantic for Type-4")


def render_semantic(donor, variant_idx, rng):
    # Fresh identifiers AND a fresh function name per copy, so a Type-4 semantic
    # clone cannot be found by grepping a shared name -- the two copies share
    # only behaviour, which is exactly what makes Type-4 hard.
    idents = fresh_idents({"idents": donor["idents"]}, rng)
    return donor["variants"][variant_idx].format(**idents)


def line_span(source):
    n = len([ln for ln in source.splitlines()])
    return 1, n


def _render_filler(mod, rng, counter):
    tmpl = rng.choice(mod.FILLER)
    return tmpl.format(fn="helper_%d" % next(counter), n=rng.randint(1, 99),
                       tag="t%d" % next(counter))


def wrap_with_filler(source, mod, rng, counter):
    """Place the needle function in its own file under a realistic name.

    Earlier versions padded files with sub-threshold filler functions, but those
    filler bodies were near-identical and an agent would report them as
    duplicates -- wasting its turns/budget and unfairly depressing its needle
    recall. With realistic VS Code-style function names a single-function module
    no longer stands out, so no filler is added.
    """
    s, e = line_span(source)
    return source, s, e


def build(out_dir, seed, lang="python", profile="default"):
    rng = random.Random(seed)
    langcfg = LANGS[lang]
    mod = importlib.import_module(langcfg["module"])
    donors = {d["name"]: d for d in mod.DONORS}
    semantic_donors = mod.SEMANTIC_DONORS
    prof = PROFILES[profile]
    filler_counter = itertools.count()
    if os.path.isdir(out_dir):
        shutil.rmtree(out_dir)
    src_root = os.path.join(out_dir, "src")
    os.makedirs(src_root)

    functions = []  # ground-truth function records
    pairs = []      # ground-truth clone pairs

    def add_copy(group_id, copy_id, clone_type, source, donor_name):
        name = langcfg["func_name"](source)
        file_text, start, end = wrap_with_filler(source, mod, rng, filler_counter)
        rel = os.path.join("src", "g%02d_c%d.%s" % (group_id, copy_id, langcfg["ext"]))
        path = os.path.join(out_dir, rel)
        with open(path, "w") as f:
            f.write(file_text)
        fid = "g%02d_c%d" % (group_id, copy_id)
        functions.append({
            "id": fid, "file": rel, "name": name,
            "start_line": start, "end_line": end,
            "group": group_id, "copy": copy_id,
            "clone_type": clone_type, "donor": donor_name,
        })
        return fid

    group_id = 0

    # Structural groups (Type-1/2/3)
    for donor_name, clone_type, copies, gap in prof:
        donor = donors[donor_name]
        ids = []
        for c in range(copies):
            # In a Type-3 group, copy 0 is a clean Type-2 baseline and the gap
            # is injected into the later copies, so the gap is what separates
            # the pair. Type-1/2 groups render every copy at their own type.
            this_gap = gap if (clone_type == 3 and c > 0) else 0
            if clone_type == 3 and c == 0:
                src = render_type2(donor, rng)
            else:
                src = render(donor, clone_type, this_gap, rng, langcfg)
            ids.append(add_copy(group_id, c, clone_type, src, donor_name))
        for i in range(len(ids)):
            for j in range(i + 1, len(ids)):
                pairs.append({"a": ids[i], "b": ids[j], "clone_type": clone_type})
        group_id += 1

    # Semantic groups (Type-4): two structurally different variants per group
    for donor in semantic_donors:
        ids = []
        for c in range(2):
            src = render_semantic(donor, c % len(donor["variants"]), rng)
            ids.append(add_copy(group_id, c, 4, src, donor["name"]))
        for i in range(len(ids)):
            for j in range(i + 1, len(ids)):
                pairs.append({"a": ids[i], "b": ids[j], "clone_type": 4})
        group_id += 1

    gt = {
        "seed": seed,
        "host": "synthetic-%s-v1" % langcfg["ext"],
        "language": lang,
        "groups": group_id,
        "functions": functions,
        "pairs": pairs,
        "clone_type_counts": _type_counts(pairs),
        "gap_profile": {name: gap for name, ct, cp, gap in prof if ct == 3},
    }
    with open(os.path.join(out_dir, "ground_truth.json"), "w") as f:
        json.dump(gt, f, indent=2)
    return gt


def _type_counts(pairs):
    counts = {}
    for p in pairs:
        counts[p["clone_type"]] = counts.get(p["clone_type"], 0) + 1
    return {str(k): counts[k] for k in sorted(counts)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--lang", default="python", choices=list(LANGS))
    ap.add_argument("--profile", default="default", choices=list(PROFILES))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    ext = LANGS[args.lang]["ext"]
    out = args.out or os.path.join(paths.data_dir(), "synthetic-%s-v1" % ext)
    out = os.path.abspath(out)
    gt = build(out, args.seed, args.lang, args.profile)
    print("corpus: %s" % out)
    print("functions: %d  groups: %d  clone pairs: %d"
          % (len(gt["functions"]), gt["groups"], len(gt["pairs"])))
    print("clone pairs by type: %s" % gt["clone_type_counts"])


if __name__ == "__main__":
    main()
