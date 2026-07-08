# Changelog

## Unreleased

- `check`: alongside each duplicate, print the import that brings the original into scope so you can delete the copy. Derived from the file path for Rust, Python, TypeScript, TSX and JavaScript; other languages print no import. Suggestion only, in text and JSON; dupehound never edits files (#9).

## 0.1.2 (2026-06-22)

- `dupehound mcp`: run as an MCP server over stdio, exposing `check` and `scan` as tools an AI coding agent can call in its loop to reuse existing code instead of rebuilding it. Local, offline, deterministic, no AI (#30).
- C# support via tree-sitter-c-sharp (#20).
- `scan --include-classes`: experimental, opt-in detection of C# classes whose property and method signatures are near-duplicates. Separate from the function clusters and never affects the slop score (#25).
- `scan`: Rust trait-impl methods (`From`, `Display`, ...) are kept out of the slop score, since each impl is required and cannot be merged (#29).

## 0.1.0 (2026-06-12)

Initial release.

- Ruby support via tree-sitter-ruby, contributed by [@paarothecoder](https://github.com/paarothecoder) (#12).

- `dupehound scan`: near-duplicate function detection across TypeScript, TSX, JavaScript, Python, Rust, Go and Java via tree-sitter extraction, token normalization and robust winnowing fingerprints (MOSS algorithm, k=10 / w=8, guarantee threshold 17 tokens). Slop score with letter grade, `--json`, `--explain`, `--card`.
- `dupehound history`: duplication-over-time chart from monthly git snapshots (no checkouts; blob-SHA fingerprint cache), inflection detection, shareable SVG/PNG card with embedded fonts.
- `dupehound check`: CI and pre-commit gate that probes changed functions against a base-revision index. Recognizes moves and in-place edits. Exit codes: 0 clean, 1 findings, 2 error.
