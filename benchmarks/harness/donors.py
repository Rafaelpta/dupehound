"""Donor function pool for the planted-clone benchmark.

Each donor is a real, self-contained Python function expressed as a template
with named identifier slots and literal slots. The generator renders a donor
into concrete source under a chosen clone type:

  Type-1  exact copy (identifiers and literals unchanged; whitespace may vary)
  Type-2  every identifier and literal systematically renamed; structure intact
  Type-3  a Type-2 copy with an injected gap (extra/removed statements)
  Type-4  semantically equivalent but written with a different structure
          (hand-authored variant templates; auto-generation of Type-4 is not
          attempted -- see METHODOLOGY.md, "Threats to validity")

Templates use ``{slot}`` placeholders. ``idents`` and ``lits`` give the donor's
own (Type-1) values for those slots. The generator supplies fresh values for
Type-2/3. Gap statements injected for Type-3 are 4-space-indented standalone
lines that reference only fresh, collision-free names.

Donors are deliberately distinct from one another: any two functions drawn from
different donors are NON-clones and serve as the benchmark's negatives.
"""

# Each donor: name, slot-default identifiers, slot-default literals, body template.
# Bodies are sized to clear dupehound's default 40-token floor.
DONORS = [
    {
        "name": "invoice_total",
        "idents": {"fn": "compute_invoice_total", "items": "items", "rate": "tax_rate",
                   "sub": "subtotal", "it": "item", "tax": "tax"},
        "lits": {"price": '"price"', "qty": '"quantity"', "zero": "0"},
        "template": (
            "def {fn}({items}, {rate}):\n"
            "    {sub} = {zero}\n"
            "    for {it} in {items}:\n"
            "        {sub} += {it}[{price}] * {it}[{qty}]\n"
            "    {tax} = {sub} * {rate}\n"
            "    return {sub} + {tax}\n"
        ),
    },
    {
        "name": "moving_average",
        "idents": {"fn": "moving_average", "seq": "series", "win": "window",
                   "out": "result", "i": "i", "chunk": "chunk", "s": "s", "v": "v"},
        "lits": {"start": "0", "one": "1"},
        "template": (
            "def {fn}({seq}, {win}):\n"
            "    {out} = []\n"
            "    for {i} in range(len({seq}) - {win} + {one}):\n"
            "        {chunk} = {seq}[{i}:{i} + {win}]\n"
            "        {s} = {start}\n"
            "        for {v} in {chunk}:\n"
            "            {s} += {v}\n"
            "        {out}.append({s} / {win})\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "validate_user",
        "idents": {"fn": "validate_user", "rec": "record", "errs": "errors",
                   "name": "name", "age": "age", "email": "email"},
        "lits": {"namek": '"name"', "agek": '"age"', "emailk": '"email"',
                 "at": '"@"', "minage": "0", "maxage": "150"},
        "template": (
            "def {fn}({rec}):\n"
            "    {errs} = []\n"
            "    if not {rec}.get({namek}):\n"
            "        {errs}.append({namek})\n"
            "    {age} = {rec}.get({agek}, {minage})\n"
            "    if {age} < {minage} or {age} > {maxage}:\n"
            "        {errs}.append({agek})\n"
            "    {email} = {rec}.get({emailk}, \"\")\n"
            "    if {at} not in {email}:\n"
            "        {errs}.append({emailk})\n"
            "    return {errs}\n"
        ),
    },
    {
        "name": "retry_call",
        "idents": {"fn": "retry_call", "func": "action", "tries": "attempts",
                   "i": "i", "exc": "err", "delay": "backoff"},
        "lits": {"start": "0", "one": "1", "base": "2"},
        "template": (
            "def {fn}({func}, {tries}):\n"
            "    {delay} = {one}\n"
            "    for {i} in range({tries}):\n"
            "        try:\n"
            "            return {func}()\n"
            "        except Exception as {exc}:\n"
            "            if {i} == {tries} - {one}:\n"
            "                raise {exc}\n"
            "            {delay} = {delay} * {base}\n"
            "    return None\n"
        ),
    },
    {
        "name": "group_by_key",
        "idents": {"fn": "group_by_key", "rows": "rows", "key": "key",
                   "out": "buckets", "row": "row", "k": "k"},
        "lits": {},
        "template": (
            "def {fn}({rows}, {key}):\n"
            "    {out} = {{}}\n"
            "    for {row} in {rows}:\n"
            "        {k} = {row}[{key}]\n"
            "        if {k} not in {out}:\n"
            "            {out}[{k}] = []\n"
            "        {out}[{k}].append({row})\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "normalize_scores",
        "idents": {"fn": "normalize_scores", "vals": "values", "lo": "low",
                   "hi": "high", "out": "scaled", "v": "v", "span": "span"},
        "lits": {"zero": "0", "one": "1.0"},
        "template": (
            "def {fn}({vals}):\n"
            "    {lo} = min({vals})\n"
            "    {hi} = max({vals})\n"
            "    {span} = {hi} - {lo}\n"
            "    if {span} == {zero}:\n"
            "        return [{one} for {v} in {vals}]\n"
            "    {out} = []\n"
            "    for {v} in {vals}:\n"
            "        {out}.append(({v} - {lo}) / {span})\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "parse_query",
        "idents": {"fn": "parse_query", "qs": "query", "out": "params",
                   "pair": "pair", "k": "k", "val": "v", "part": "part"},
        "lits": {"amp": '"&"', "eq": '"="', "empty": '""'},
        "template": (
            "def {fn}({qs}):\n"
            "    {out} = {{}}\n"
            "    for {part} in {qs}.split({amp}):\n"
            "        if {eq} not in {part}:\n"
            "            continue\n"
            "        {k}, {val} = {part}.split({eq}, 1)\n"
            "        {out}[{k}] = {val}\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "balance_brackets",
        "idents": {"fn": "is_balanced", "text": "text", "stack": "stack",
                   "ch": "ch", "pairs": "pairs", "top": "top"},
        "lits": {},
        "template": (
            "def {fn}({text}):\n"
            "    {stack} = []\n"
            "    {pairs} = {{\")\": \"(\", \"]\": \"[\", \"}}\": \"{{\"}}\n"
            "    for {ch} in {text}:\n"
            "        if {ch} in \"([{{\":\n"
            "            {stack}.append({ch})\n"
            "        elif {ch} in {pairs}:\n"
            "            if not {stack} or {stack}.pop() != {pairs}[{ch}]:\n"
            "                return False\n"
            "    return not {stack}\n"
        ),
    },
    {
        "name": "merge_sorted",
        "idents": {"fn": "merge_sorted", "a": "left", "b": "right",
                   "out": "merged", "i": "i", "j": "j"},
        "lits": {"zero": "0", "one": "1"},
        "template": (
            "def {fn}({a}, {b}):\n"
            "    {out} = []\n"
            "    {i} = {zero}\n"
            "    {j} = {zero}\n"
            "    while {i} < len({a}) and {j} < len({b}):\n"
            "        if {a}[{i}] <= {b}[{j}]:\n"
            "            {out}.append({a}[{i}])\n"
            "            {i} += {one}\n"
            "        else:\n"
            "            {out}.append({b}[{j}])\n"
            "            {j} += {one}\n"
            "    {out}.extend({a}[{i}:])\n"
            "    {out}.extend({b}[{j}:])\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "rate_limiter",
        "idents": {"fn": "allow_request", "log": "timestamps", "now": "now",
                   "limit": "limit", "window": "window", "t": "t", "recent": "recent"},
        "lits": {},
        "template": (
            "def {fn}({log}, {now}, {limit}, {window}):\n"
            "    {recent} = [{t} for {t} in {log} if {now} - {t} < {window}]\n"
            "    if len({recent}) >= {limit}:\n"
            "        return False, {recent}\n"
            "    {recent}.append({now})\n"
            "    return True, {recent}\n"
        ),
    },
    {
        "name": "flatten_tree",
        "idents": {"fn": "flatten", "node": "node", "out": "acc",
                   "child": "child", "kids": "children"},
        "lits": {"valk": '"value"', "kidsk": '"children"'},
        "template": (
            "def {fn}({node}, {out}):\n"
            "    {out}.append({node}[{valk}])\n"
            "    {kids} = {node}.get({kidsk}, [])\n"
            "    for {child} in {kids}:\n"
            "        {fn}({child}, {out})\n"
            "    return {out}\n"
        ),
    },
    {
        "name": "histogram",
        "idents": {"fn": "histogram", "vals": "values", "size": "bucket_size",
                   "out": "bins", "v": "v", "key": "key"},
        "lits": {"zero": "0", "one": "1"},
        "template": (
            "def {fn}({vals}, {size}):\n"
            "    {out} = {{}}\n"
            "    for {v} in {vals}:\n"
            "        {key} = ({v} // {size}) * {size}\n"
            "        if {key} not in {out}:\n"
            "            {out}[{key}] = {zero}\n"
            "        {out}[{key}] += {one}\n"
            "    return {out}\n"
        ),
    },
]

# Sub-threshold filler functions used to pad needle files (see donors_ts.FILLER).
FILLER = [
    "def {fn}():\n    return {n}\n",
    "def {fn}(x):\n    return x + {n}\n",
    "def {fn}(s):\n    return s.strip()\n",
    "def {fn}(a, b):\n    return a if a > b else b\n",
    "def {fn}(xs):\n    return len(xs)\n",
]


# Type-4 donors: each provides two semantically equivalent but structurally
# different variant templates. A Type-4 group alternates between the variants,
# so every within-group pair is a true semantic clone that a token/structure
# detector is expected to MISS. This is where LLMs are expected to win, and
# including it is what keeps the benchmark honest about dupehound's scope.
SEMANTIC_DONORS = [
    {
        "name": "sum_squares",
        "variants": [
            (
                "def {fn}({xs}):\n"
                "    {total} = 0\n"
                "    for {x} in {xs}:\n"
                "        {total} += {x} * {x}\n"
                "    return {total}\n"
            ),
            (
                "def {fn}({xs}):\n"
                "    return sum({x} ** 2 for {x} in {xs})\n"
            ),
        ],
        "idents": {"fn": "sum_of_squares", "xs": "numbers", "total": "total", "x": "n"},
    },
    {
        "name": "factorial",
        "variants": [
            (
                "def {fn}({n}):\n"
                "    {acc} = 1\n"
                "    for {i} in range(2, {n} + 1):\n"
                "        {acc} *= {i}\n"
                "    return {acc}\n"
            ),
            (
                "def {fn}({n}):\n"
                "    if {n} <= 1:\n"
                "        return 1\n"
                "    return {n} * {fn}({n} - 1)\n"
            ),
        ],
        "idents": {"fn": "factorial", "n": "n", "acc": "result", "i": "i"},
    },
    {
        "name": "count_words",
        "variants": [
            (
                "def {fn}({text}):\n"
                "    {out} = {{}}\n"
                "    for {w} in {text}.split():\n"
                "        {out}[{w}] = {out}.get({w}, 0) + 1\n"
                "    return {out}\n"
            ),
            (
                "def {fn}({text}):\n"
                "    from collections import Counter\n"
                "    return dict(Counter({text}.split()))\n"
            ),
        ],
        "idents": {"fn": "word_counts", "text": "text", "out": "counts", "w": "word"},
    },
]
