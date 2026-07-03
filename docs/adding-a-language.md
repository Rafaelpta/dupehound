# Adding a language

dupehound is grammar-driven. To support a language you give it two things: a
tree-sitter grammar (a Rust crate) and a small query (`.scm`) that captures each
function and its body. Everything downstream (normalization, fingerprinting,
clustering) is language-agnostic and needs no changes.

Adding a language is the most wanted contribution and one of the smallest. This
is the full walkthrough; `CONTRIBUTING.md` has the short version.

## The pieces

- `Cargo.toml`: the `tree-sitter-<lang>` grammar crate.
- `src/lang/mod.rs`: the `Lang` enum and the functions that map it to a grammar
  and a query.
- `src/lang/queries/<lang>.scm`: the query that captures functions.
- `src/walk.rs`: the file-extension list.
- `tests/languages.rs`: a renamed-clone fixture that proves it works.
- `README.md`: the `Languages:` line.

## Steps

### 1. Add the grammar crate

Add the tree-sitter grammar to `Cargo.toml`:

```toml
tree-sitter-elixir = "0.3"
```

Pick a version whose parser ABI is in tree-sitter's supported range. If it is
too new or too old the `abi_in_supported_range` test fails with a clear message;
bump or pin the crate until it passes.

### 2. Register the language in `src/lang/mod.rs`

Add a variant everywhere the `Lang` enum is matched. There are a few spots, and
three of them are fixed-size arrays whose length you must bump. Missing one is
the only real gotcha, so here is the full list:

1. The `Lang` enum: add your variant.
2. `pub const ALL: [Lang; 14]`: add your variant **and bump `14` to `15`**.
3. `from_path`: map the file extension(s) to your variant.
4. `name`: the human-readable name.
5. `language`: return the grammar, usually `tree_sitter_<lang>::LANGUAGE.into()`
   (some crates expose a function like `language_php()` instead; check the crate).
6. `query_source`: `include_str!("queries/<lang>.scm")`.
7. `query`: bump `static QUERIES: [OnceLock<Query>; 14]` to `15`.
8. `shape_query`: bump `static SHAPE_QUERIES: [OnceLock<Query>; 14]` to `15` (the
   shape query itself is optional and C#-only for now; you do not need one).

If the project has moved past 14 languages by the time you read this, the arrays
will already be a different number; just keep them all equal to the count in
`ALL`.

### 3. Write the query `src/lang/queries/<lang>.scm`

The query captures three things per function: the whole function node as `@func`,
its name as `@name`, and its body as `@body`. The simplest form, from
`python.scm`:

```scheme
(function_definition
  name: (identifier) @name
  body: (block) @body) @func
```

Rules that matter:

- **Constrain `@name` to the identifier node** (`name: (identifier) @name`), not a
  wildcard. An unconstrained name can match more than once and report the same
  function twice.
- **Cover every function form** the language has: free functions, methods,
  associated functions, constructors, lambdas if they are named. Add one pattern
  per form in the same file; they accumulate.
- You only need `@func` and `@body`; `@name` is used for reporting. If the
  grammar has no distinct body node, capture the largest node that is the body.

To find the right node names, parse a sample file with the tree-sitter CLI
(`tree-sitter parse file.ext`) or use the online playground for that grammar, and
read the node kinds it prints. `go.scm` is a good minimal example with a method
form; `csharp.scm` shows several forms in one file.

### 4. Classification (usually nothing to do)

Normalization decides what each leaf token becomes (identifiers and literals are
folded to sentinels so renames do not matter). This is driven by `classify` in
`src/lang/mod.rs` using substring rules on node kind names (anything containing
`identifier`, `string`, `number`, and so on). These conventions hold across the
bundled grammars, so most languages need no change. Only touch `classify` if your
grammar names identifier or literal nodes in an unusual way; if it does, add the
kind name to the matching branch.

### 5. Add the extension to `src/walk.rs`

Add your file extension to the supported-extensions list so the "no supported
source files" message stays accurate.

### 6. Add a fixture to `tests/languages.rs`

Add a `Fixture` with a **renamed (Type-2) clone pair**: the same function written
twice with every identifier renamed. This is the real test; it proves the query
captures the function and that a renamed copy fingerprints identically. Copy an
existing fixture (for example `PHP`) and change the source, the file names, and
the two function names.

### 7. Add the language to `README.md`

Add it to the `Languages:` line.

## Verify

```sh
cargo test        # queries_compile, abi range, and your new golden fixture
cargo clippy --all-targets
cargo fmt --check
```

If the suite is green, you are done. Open a PR with the fixture; the fixture is
what makes it reviewable.

## Troubleshooting

- **`bad <Lang> query` panic at startup.** The query references a node name the
  grammar does not have. Parse a sample file and check the exact node kinds; names
  differ between grammars (for example `function_definition` vs
  `function_declaration`).
- **A function is reported twice.** `@name` is probably unconstrained or two
  patterns both match the same node. Constrain `@name` to the identifier node.
- **`abi_in_supported_range` fails.** The grammar crate's parser ABI is outside
  the range tree-sitter supports here. Pin the crate to a compatible version.
- **A renamed pair does not cluster.** Confirm `@body` captures the real body
  (not an empty or wrapper node), and that the function clears the default
  `--min-tokens` size; very short functions are skipped by design.

## Scope note: structural detection only

dupehound fingerprints the structure of function bodies. Detecting near-duplicate
*type shapes* (for example C# classes with similar property and method
signatures) is a separate, opt-in mode behind `--include-classes`, wired through
`shape_query` and `queries/*_shape.scm`. It is experimental and currently C#-only;
adding a shape query for another language follows the same pattern as the function
query but is not required to support a language.
