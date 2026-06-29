"""LLM system adapter for the benchmark.

Frames clone detection as a repo-scale discovery task: the model is shown every
candidate function (id + source) and asked to return the set of clone pairs as
JSON. The same prompt and parser are used for every backend, so the only thing
that varies between models is the model itself.

Backends:
  mock       deterministic stand-in that emits the real JSON response format so
             the parser is exercised end to end. Recall/precision/cost/latency
             follow a per-model profile. Results from this backend are clearly
             labelled SIMULATED -- they prove the harness, not the models.
  anthropic  real Claude call (Messages API, stdlib urllib). Needs ANTHROPIC_API_KEY.
  openai     real GPT call (Chat Completions). Needs OPENAI_API_KEY.

Token counts from real backends come from the provider's usage field. For the
mock and for any provider that omits usage we fall back to a chars/4 heuristic,
which is disclosed in the results file.
"""
import hashlib
import json
import os
import random
import re

# Per-model profiles. Prices are USD per 1M tokens (input, output) and MUST be
# set to each provider's current published price before a real run; the values
# here drive the SIMULATED mock demonstration only. `recall` is the per-clone-
# type detection probability the mock targets; `fp_rate` is the chance it
# invents a pair from a non-clone candidate pair.
MODELS = {
    "mock:frontier-a": {"backend": "mock", "simulated": True,
                        "recall": {1: 1.00, 2: 0.98, 3: 0.78, 4: 0.58},
                        "fp_rate": 0.015, "price_in": 15.0, "price_out": 75.0,
                        "ms_per_ktok_in": 110, "ms_per_tok_out": 8},
    "mock:frontier-b": {"backend": "mock", "simulated": True,
                        "recall": {1: 1.00, 2: 0.95, 3: 0.62, 4: 0.46},
                        "fp_rate": 0.025, "price_in": 3.0, "price_out": 15.0,
                        "ms_per_ktok_in": 70, "ms_per_tok_out": 6},
    "mock:frontier-c": {"backend": "mock", "simulated": True,
                        "recall": {1: 0.98, 2: 0.90, 3: 0.55, 4: 0.40},
                        "fp_rate": 0.035, "price_in": 2.5, "price_out": 10.0,
                        "ms_per_ktok_in": 60, "ms_per_tok_out": 5},
    "mock:cheap-open": {"backend": "mock", "simulated": True,
                        "recall": {1: 0.92, 2: 0.80, 3: 0.35, 4: 0.20},
                        "fp_rate": 0.075, "price_in": 0.20, "price_out": 0.20,
                        "ms_per_ktok_in": 45, "ms_per_tok_out": 4},
    # Real models: fill price_in/price_out from the provider's pricing page.
    "claude-opus-4-8": {"backend": "anthropic", "simulated": False,
                        "api_model": "claude-opus-4-8", "price_in": None, "price_out": None},
    "gpt-frontier": {"backend": "openai", "simulated": False,
                    "api_model": "gpt-5", "price_in": None, "price_out": None},
}

SYSTEM_PROMPT = (
    "You are a precise code-duplication detector. You will be given a list of "
    "functions, each with an ID and its source. Identify every pair of functions "
    "that are clones of each other (duplicated logic), including pairs that have "
    "been renamed, lightly edited, or reimplemented with equivalent logic. Do not "
    "report a pair unless you are confident the two functions duplicate each other.\n\n"
    "Respond with ONLY a JSON object of the form: "
    '{"pairs": [["ID1","ID2"], ["ID3","ID4"]]}. No prose.'
)


def approx_tokens(text):
    return max(1, len(text) // 4)


def build_prompt(corpus_dir, gt):
    blocks = []
    for fn in gt["functions"]:
        with open(os.path.join(corpus_dir, fn["file"])) as f:
            src = f.read().rstrip()
        blocks.append("### %s\n%s" % (fn["id"], src))
    user = ("Here are the functions. Return all clone pairs as JSON.\n\n"
            + "\n\n".join(blocks))
    return SYSTEM_PROMPT, user


def parse_response(text, valid_ids):
    """Extract clone pairs from a model response, tolerant of stray prose."""
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if not m:
        return set()
    try:
        obj = json.loads(m.group(0))
    except json.JSONDecodeError:
        return set()
    out = set()
    for pair in obj.get("pairs", []):
        if not isinstance(pair, (list, tuple)) or len(pair) != 2:
            continue
        a, b = str(pair[0]), str(pair[1])
        if a in valid_ids and b in valid_ids and a != b:
            out.add(frozenset((a, b)))
    return out


def _stable_seed(model, run_idx):
    h = hashlib.sha1(("%s:%d" % (model, run_idx)).encode()).digest()
    return int.from_bytes(h[:8], "big")


def _mock_generate(corpus_dir, gt, model, run_idx):
    """Emit a response in the real JSON format, with per-type recall + noise."""
    prof = MODELS[model]
    rng = random.Random(_stable_seed(model, run_idx))
    func_ids = [f["id"] for f in gt["functions"]]
    true_pairs = {frozenset((p["a"], p["b"])): p["clone_type"] for p in gt["pairs"]}

    predicted = []
    for pair, ct in true_pairs.items():
        if rng.random() < prof["recall"].get(ct, 0.0):
            predicted.append(sorted(pair))
    # false positives over non-clone candidate pairs
    for i in range(len(func_ids)):
        for j in range(i + 1, len(func_ids)):
            fp = frozenset((func_ids[i], func_ids[j]))
            if fp in true_pairs:
                continue
            if rng.random() < prof["fp_rate"]:
                predicted.append(sorted(fp))
    return json.dumps({"pairs": predicted})


def _anthropic_generate(system, user, api_model):
    import urllib.request
    key = os.environ["ANTHROPIC_API_KEY"]
    body = json.dumps({
        "model": api_model, "max_tokens": 4096, "temperature": 0,
        "system": system, "messages": [{"role": "user", "content": user}],
    }).encode()
    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages", data=body,
        headers={"x-api-key": key, "anthropic-version": "2023-06-01",
                 "content-type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        out = json.loads(resp.read())
    text = "".join(b.get("text", "") for b in out.get("content", []))
    usage = out.get("usage", {})
    return text, usage.get("input_tokens"), usage.get("output_tokens")


def _openai_generate(system, user, api_model):
    import urllib.request
    key = os.environ["OPENAI_API_KEY"]
    body = json.dumps({
        "model": api_model, "temperature": 0,
        "messages": [{"role": "system", "content": system},
                     {"role": "user", "content": user}],
    }).encode()
    req = urllib.request.Request(
        "https://api.openai.com/v1/chat/completions", data=body,
        headers={"authorization": "Bearer %s" % key, "content-type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        out = json.loads(resp.read())
    text = out["choices"][0]["message"]["content"]
    usage = out.get("usage", {})
    return text, usage.get("prompt_tokens"), usage.get("completion_tokens")


def run(corpus_dir, gt, model, run_idx=0):
    if model not in MODELS:
        raise ValueError("unknown model %r; known: %s" % (model, list(MODELS)))
    prof = MODELS[model]
    system, user = build_prompt(corpus_dir, gt)
    valid_ids = {f["id"] for f in gt["functions"]}
    in_tok = approx_tokens(system + user)

    import time
    t0 = time.perf_counter()
    if prof["backend"] == "mock":
        text = _mock_generate(corpus_dir, gt, model, run_idx)
        out_tok = approx_tokens(text)
        # simulate latency deterministically rather than sleeping
        wall_ms = (in_tok / 1000.0) * prof["ms_per_ktok_in"] + out_tok * prof["ms_per_tok_out"]
    elif prof["backend"] == "anthropic":
        text, ai, ao = _anthropic_generate(system, user, prof["api_model"])
        in_tok = ai or in_tok
        out_tok = ao or approx_tokens(text)
        wall_ms = (time.perf_counter() - t0) * 1000.0
    elif prof["backend"] == "openai":
        text, ai, ao = _openai_generate(system, user, prof["api_model"])
        in_tok = ai or in_tok
        out_tok = ao or approx_tokens(text)
        wall_ms = (time.perf_counter() - t0) * 1000.0
    else:
        raise ValueError("bad backend %r" % prof["backend"])

    predicted = parse_response(text, valid_ids)
    price_in, price_out = prof.get("price_in"), prof.get("price_out")
    usd = None
    if price_in is not None and price_out is not None:
        usd = in_tok / 1e6 * price_in + out_tok / 1e6 * price_out

    return {
        "system": model,
        "kind": "llm",
        "simulated": prof.get("simulated", False),
        "predicted_pairs": predicted,
        "telemetry": {
            "wall_ms": wall_ms,
            "tokens_in": in_tok,
            "tokens_out": out_tok,
            "usd": usd,
        },
        "config": {"backend": prof["backend"], "run_idx": run_idx},
    }
