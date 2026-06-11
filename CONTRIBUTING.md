# Contributing to dupehound

Thanks for sniffing around! 🐕

## Getting started

```sh
git clone https://github.com/GITHUB_USER/dupehound && cd dupehound
cargo test        # full suite: unit, golden clone fixtures, CLI integration
cargo run -- scan .
```

Stable Rust is all you need. First build compiles the tree-sitter grammars
(C code), so it takes a minute; after that it's fast.

## Adding a language

This is the most wanted contribution and it's genuinely small:

1. Add the `tree-sitter-<lang>` crate to `Cargo.toml`.
2. Add a variant to `Lang` in `src/lang/mod.rs` (extension mapping, language
   fn, query path).
3. Write `src/lang/queries/<lang>.scm` capturing `@func`, `@name`, `@body`
   for the language's function/method forms (look at `go.scm` for a minimal
   example).
4. Add a renamed-clone fixture pair to `tests/languages.rs`.

If `cargo test` passes — including the ABI range check and query compilation
test — you're done.

## Ground rules

- **No network calls. No AI. No telemetry.** The tool's whole identity is
  that it's deterministic and local; PRs that break this won't be merged.
- New matching behavior needs a fixture test (positive *and* negative — the
  false-positive tests are the most valuable ones).
- `cargo clippy --all-targets` must be clean.
- Keep the report output stable: anything consumed by scripts goes through
  `--json` (versioned schema), not by parsing the pretty output.

## Reporting false positives

Best possible issue: a small self-contained code pair that dupehound matches
but shouldn't (or vice versa), plus the `--explain` output. These directly
become regression fixtures.
