# Changelog

## 0.1.0 — unreleased

Initial release.

- `dupehound scan` — near-duplicate function detection across TypeScript,
  TSX, JavaScript, Python, Rust, Go and Java via tree-sitter extraction,
  token normalization and robust winnowing fingerprints (MOSS algorithm,
  k=10 / w=8, guarantee threshold 17 tokens). Slop score with letter grade,
  `--json`, `--explain`, `--card`.
- `dupehound history` — duplication-over-time chart from monthly git
  snapshots (no checkouts; blob-SHA fingerprint cache), inflection
  detection, shareable SVG/PNG card with embedded fonts.
- `dupehound check` — CI/pre-commit gate that probes changed functions
  against a base-revision index; recognizes moves and in-place edits;
  exit code contract (0 clean / 1 findings / 2 error).
