# Contributing to dupehound

## Getting started

```sh
git clone https://github.com/Rafaelpta/dupehound && cd dupehound
cargo test        # full suite: unit, golden clone fixtures, CLI integration
cargo run -- scan .
```

Stable Rust is all you need. The first build compiles the tree-sitter grammars (C code), so it takes a minute. After that it's fast.

## Adding a language

The most wanted contribution, and a small one. The short version:

1. Add the `tree-sitter-<lang>` crate to `Cargo.toml`.
2. Add a variant to `Lang` in `src/lang/mod.rs` (extension mapping, language fn, query path). Bump the fixed-size arrays (`ALL` and the two query caches) to match the new count.
3. Write `src/lang/queries/<lang>.scm` capturing `@func`, `@name` and `@body` for the language's function and method forms. `go.scm` is a minimal example. Constrain `@name` to the identifier node (e.g. `name: (identifier) @name`), not a wildcard, or a function can match more than once.
4. Add a renamed-clone fixture pair to `tests/languages.rs`.
5. Add the file extension to the "no supported source files" message in `src/walk.rs`.
6. Add the language to the `Languages:` line in `README.md`.

If `cargo test` passes, including the ABI range check and the query compilation test, you're done.

**Full walkthrough**, with the query details, the array-size gotcha, how to find grammar node names, and troubleshooting: [docs/adding-a-language.md](docs/adding-a-language.md).

## Ground rules

- No network calls, no AI, no telemetry. The tool's identity is that it's deterministic and local. PRs that break this won't be merged.
- New matching behavior needs a fixture test, positive and negative. The false-positive tests are the most valuable ones.
- `cargo clippy --all-targets` must be clean.
- Anything consumed by scripts goes through `--json` (versioned schema), not by parsing the pretty output.

## Reporting false positives

The best possible issue: a small self-contained code pair that dupehound matches but shouldn't (or the reverse), plus the `--explain` output. These become regression fixtures directly.
