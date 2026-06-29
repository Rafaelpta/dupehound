"""TypeScript donor pool -- the TS counterpart of donors.py.

Same donor names, same identifier/literal slots, so the language-agnostic
planting logic in plant.py (groups, clone types, renaming, gaps) works
unchanged. Templates are idiomatic TypeScript. Literal braces are escaped as
``{{`` / ``}}`` because plant.py renders templates with str.format. Every
structural donor's first line is ``function NAME(...`` so the TS name parser
can read it.
"""

DONORS = [
    {
        "name": "invoice_total",
        "idents": {"fn": "computeInvoiceTotal", "items": "items", "rate": "taxRate",
                   "sub": "subtotal", "it": "item", "tax": "tax"},
        "lits": {"price": '"price"', "qty": '"quantity"', "zero": "0"},
        "template": (
            "function {fn}({items}: any[], {rate}: number): number {{\n"
            "  let {sub} = {zero};\n"
            "  for (const {it} of {items}) {{\n"
            "    {sub} += {it}[{price}] * {it}[{qty}];\n"
            "  }}\n"
            "  const {tax} = {sub} * {rate};\n"
            "  return {sub} + {tax};\n"
            "}}\n"
        ),
    },
    {
        "name": "moving_average",
        "idents": {"fn": "movingAverage", "seq": "series", "win": "window",
                   "out": "result", "i": "i", "chunk": "chunk", "s": "s", "v": "v"},
        "lits": {"start": "0"},
        "template": (
            "function {fn}({seq}: number[], {win}: number): number[] {{\n"
            "  const {out}: number[] = [];\n"
            "  for (let {i} = 0; {i} <= {seq}.length - {win}; {i}++) {{\n"
            "    const {chunk} = {seq}.slice({i}, {i} + {win});\n"
            "    let {s} = {start};\n"
            "    for (const {v} of {chunk}) {{\n"
            "      {s} += {v};\n"
            "    }}\n"
            "    {out}.push({s} / {win});\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
    {
        "name": "validate_user",
        "idents": {"fn": "validateUser", "rec": "record", "errs": "errors",
                   "age": "age", "email": "email"},
        "lits": {"namek": '"name"', "agek": '"age"', "emailk": '"email"',
                 "at": '"@"', "minage": "0", "maxage": "150"},
        "template": (
            "function {fn}({rec}: any): string[] {{\n"
            "  const {errs}: string[] = [];\n"
            "  if (!{rec}[{namek}]) {{\n"
            "    {errs}.push({namek});\n"
            "  }}\n"
            "  const {age} = {rec}[{agek}] ?? {minage};\n"
            "  if ({age} < {minage} || {age} > {maxage}) {{\n"
            "    {errs}.push({agek});\n"
            "  }}\n"
            "  const {email} = {rec}[{emailk}] ?? \"\";\n"
            "  if (!{email}.includes({at})) {{\n"
            "    {errs}.push({emailk});\n"
            "  }}\n"
            "  return {errs};\n"
            "}}\n"
        ),
    },
    {
        "name": "retry_call",
        "idents": {"fn": "retryCall", "func": "action", "tries": "attempts",
                   "i": "i", "exc": "err", "delay": "backoff"},
        "lits": {"one": "1", "base": "2"},
        "template": (
            "function {fn}({func}: () => any, {tries}: number): any {{\n"
            "  let {delay} = {one};\n"
            "  for (let {i} = 0; {i} < {tries}; {i}++) {{\n"
            "    try {{\n"
            "      return {func}();\n"
            "    }} catch ({exc}) {{\n"
            "      if ({i} === {tries} - {one}) {{\n"
            "        throw {exc};\n"
            "      }}\n"
            "      {delay} = {delay} * {base};\n"
            "    }}\n"
            "  }}\n"
            "  return null;\n"
            "}}\n"
        ),
    },
    {
        "name": "group_by_key",
        "idents": {"fn": "groupByKey", "rows": "rows", "key": "key",
                   "out": "buckets", "row": "row", "k": "k"},
        "lits": {},
        "template": (
            "function {fn}({rows}: any[], {key}: string): Record<string, any[]> {{\n"
            "  const {out}: Record<string, any[]> = {{}};\n"
            "  for (const {row} of {rows}) {{\n"
            "    const {k} = {row}[{key}];\n"
            "    if (!({k} in {out})) {{\n"
            "      {out}[{k}] = [];\n"
            "    }}\n"
            "    {out}[{k}].push({row});\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
    {
        "name": "normalize_scores",
        "idents": {"fn": "normalizeScores", "vals": "values", "lo": "low",
                   "hi": "high", "out": "scaled", "v": "v", "span": "span"},
        "lits": {"zero": "0", "one": "1.0"},
        "template": (
            "function {fn}({vals}: number[]): number[] {{\n"
            "  const {lo} = Math.min(...{vals});\n"
            "  const {hi} = Math.max(...{vals});\n"
            "  const {span} = {hi} - {lo};\n"
            "  if ({span} === {zero}) {{\n"
            "    return {vals}.map(({v}) => {one});\n"
            "  }}\n"
            "  const {out}: number[] = [];\n"
            "  for (const {v} of {vals}) {{\n"
            "    {out}.push(({v} - {lo}) / {span});\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
    {
        "name": "parse_query",
        "idents": {"fn": "parseQuery", "qs": "query", "out": "params",
                   "k": "k", "val": "v", "part": "part"},
        "lits": {"amp": '"&"', "eq": '"="'},
        "template": (
            "function {fn}({qs}: string): Record<string, string> {{\n"
            "  const {out}: Record<string, string> = {{}};\n"
            "  for (const {part} of {qs}.split({amp})) {{\n"
            "    if (!{part}.includes({eq})) {{\n"
            "      continue;\n"
            "    }}\n"
            "    const [{k}, {val}] = {part}.split({eq});\n"
            "    {out}[{k}] = {val};\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
    {
        "name": "balance_brackets",
        "idents": {"fn": "isBalanced", "text": "text", "stack": "stack",
                   "ch": "ch", "pairs": "pairs"},
        "lits": {},
        "template": (
            "function {fn}({text}: string): boolean {{\n"
            "  const {stack}: string[] = [];\n"
            "  const {pairs}: Record<string, string> = {{ \")\": \"(\", \"]\": \"[\", \"}}\": \"{{\" }};\n"
            "  for (const {ch} of {text}) {{\n"
            "    if (\"([{{\".includes({ch})) {{\n"
            "      {stack}.push({ch});\n"
            "    }} else if ({ch} in {pairs}) {{\n"
            "      if ({stack}.length === 0 || {stack}.pop() !== {pairs}[{ch}]) {{\n"
            "        return false;\n"
            "      }}\n"
            "    }}\n"
            "  }}\n"
            "  return {stack}.length === 0;\n"
            "}}\n"
        ),
    },
    {
        "name": "merge_sorted",
        "idents": {"fn": "mergeSorted", "a": "left", "b": "right",
                   "out": "merged", "i": "i", "j": "j"},
        "lits": {"zero": "0", "one": "1"},
        "template": (
            "function {fn}({a}: number[], {b}: number[]): number[] {{\n"
            "  const {out}: number[] = [];\n"
            "  let {i} = {zero};\n"
            "  let {j} = {zero};\n"
            "  while ({i} < {a}.length && {j} < {b}.length) {{\n"
            "    if ({a}[{i}] <= {b}[{j}]) {{\n"
            "      {out}.push({a}[{i}]);\n"
            "      {i} += {one};\n"
            "    }} else {{\n"
            "      {out}.push({b}[{j}]);\n"
            "      {j} += {one};\n"
            "    }}\n"
            "  }}\n"
            "  return {out}.concat({a}.slice({i})).concat({b}.slice({j}));\n"
            "}}\n"
        ),
    },
    {
        "name": "rate_limiter",
        "idents": {"fn": "allowRequest", "log": "timestamps", "now": "now",
                   "limit": "limit", "window": "window", "t": "t", "recent": "recent"},
        "lits": {},
        "template": (
            "function {fn}({log}: number[], {now}: number, {limit}: number, {window}: number): [boolean, number[]] {{\n"
            "  const {recent} = {log}.filter(({t}) => {now} - {t} < {window});\n"
            "  if ({recent}.length >= {limit}) {{\n"
            "    return [false, {recent}];\n"
            "  }}\n"
            "  {recent}.push({now});\n"
            "  return [true, {recent}];\n"
            "}}\n"
        ),
    },
    {
        "name": "flatten_tree",
        "idents": {"fn": "flatten", "node": "node", "out": "acc",
                   "child": "child", "kids": "children"},
        "lits": {"valk": '"value"', "kidsk": '"children"'},
        "template": (
            "function {fn}({node}: any, {out}: any[]): any[] {{\n"
            "  {out}.push({node}[{valk}]);\n"
            "  const {kids} = {node}[{kidsk}] ?? [];\n"
            "  for (const {child} of {kids}) {{\n"
            "    {fn}({child}, {out});\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
    {
        "name": "histogram",
        "idents": {"fn": "histogram", "vals": "values", "size": "bucketSize",
                   "out": "bins", "v": "v", "key": "key"},
        "lits": {"zero": "0", "one": "1"},
        "template": (
            "function {fn}({vals}: number[], {size}: number): Record<number, number> {{\n"
            "  const {out}: Record<number, number> = {{}};\n"
            "  for (const {v} of {vals}) {{\n"
            "    const {key} = Math.floor({v} / {size}) * {size};\n"
            "    if (!({key} in {out})) {{\n"
            "      {out}[{key}] = {zero};\n"
            "    }}\n"
            "    {out}[{key}] += {one};\n"
            "  }}\n"
            "  return {out};\n"
            "}}\n"
        ),
    },
]

# Sub-threshold filler functions (each well under dupehound's min-tokens, so
# dupehound never extracts them and they can never become accidental clones).
# They pad needle files so a planted needle sits inside an ordinary-looking small
# module instead of a tell-tale one-function file. Rendered with a unique name
# per instance so an agent cannot pair files by shared filler.
FILLER = [
    "function {fn}(): number {{\n  return {n};\n}}\n",
    "function {fn}(x: number): number {{\n  return x + {n};\n}}\n",
    "function {fn}(s: string): string {{\n  return s.trim();\n}}\n",
    "function {fn}(a: number, b: number): number {{\n  return a > b ? a : b;\n}}\n",
    "const {fn} = (): boolean => {n} > 0;\n",
    "function {fn}(xs: number[]): number {{\n  return xs.length;\n}}\n",
    "function {fn}(v: unknown): boolean {{\n  return v != null;\n}}\n",
]


SEMANTIC_DONORS = [
    {
        "name": "sum_squares",
        "variants": [
            (
                "function {fn}({xs}: number[]): number {{\n"
                "  let {total} = 0;\n"
                "  for (const {x} of {xs}) {{\n"
                "    {total} += {x} * {x};\n"
                "  }}\n"
                "  return {total};\n"
                "}}\n"
            ),
            (
                "function {fn}({xs}: number[]): number {{\n"
                "  return {xs}.reduce((a, {x}) => a + {x} ** 2, 0);\n"
                "}}\n"
            ),
        ],
        "idents": {"fn": "sumOfSquares", "xs": "numbers", "total": "total", "x": "n"},
    },
    {
        "name": "factorial",
        "variants": [
            (
                "function {fn}({n}: number): number {{\n"
                "  let {acc} = 1;\n"
                "  for (let {i} = 2; {i} <= {n}; {i}++) {{\n"
                "    {acc} *= {i};\n"
                "  }}\n"
                "  return {acc};\n"
                "}}\n"
            ),
            (
                "function {fn}({n}: number): number {{\n"
                "  if ({n} <= 1) {{\n"
                "    return 1;\n"
                "  }}\n"
                "  return {n} * {fn}({n} - 1);\n"
                "}}\n"
            ),
        ],
        "idents": {"fn": "factorial", "n": "n", "acc": "result", "i": "i"},
    },
    {
        "name": "count_words",
        "variants": [
            (
                "function {fn}({text}: string): Record<string, number> {{\n"
                "  const {out}: Record<string, number> = {{}};\n"
                "  for (const {w} of {text}.split(\" \")) {{\n"
                "    {out}[{w}] = ({out}[{w}] ?? 0) + 1;\n"
                "  }}\n"
                "  return {out};\n"
                "}}\n"
            ),
            (
                "function {fn}({text}: string): Record<string, number> {{\n"
                "  const {out}: Record<string, number> = {{}};\n"
                "  {text}.split(\" \").forEach(({w}) => {{\n"
                "    {out}[{w}] = ({out}[{w}] || 0) + 1;\n"
                "  }});\n"
                "  return {out};\n"
                "}}\n"
            ),
        ],
        "idents": {"fn": "wordCounts", "text": "text", "out": "counts", "w": "word"},
    },
]
