# Running dupehound in CI

## GitHub Actions

```yaml
# .github/workflows/dupehound.yml
name: dupehound
on: pull_request
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0   # check needs the merge-base
      - name: Install dupehound
        run: |
          curl -sL https://github.com/Rafaelpta/dupehound/releases/latest/download/dupehound-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv dupehound /usr/local/bin/
      - name: Block new duplicates
        run: dupehound check --diff origin/${{ github.base_ref }} .
```

`check` compares against the merge-base of the given revision and HEAD,
which matches pull request semantics. Exit code 1 fails the job when a
newly added function duplicates existing code; the log line names the
original to reuse.

## Pre-commit

Run `dupehound check .` with staged changes. If anything is staged it
compares the index against HEAD, otherwise the working tree against
HEAD. Untracked files are included.

## Coding agents

The check output is designed to be fed back to the agent that triggered
it. In `CLAUDE.md` or `AGENTS.md`:

```markdown
Before committing, run `dupehound check .`. If it reports that a function
you wrote duplicates existing code, delete your version and reuse the
original at the reported location.
```

For scripted use, `--json` emits a versioned schema with one finding per
new duplicate, including file, line, similarity and the location of the
original.
