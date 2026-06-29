"""Where generated corpora live.

dupehound honors .gitignore, so corpora cannot sit inside the repo (the repo's
ignore rules would hide them from the scanner too). Generated, regenerable
corpora therefore live OUTSIDE the repo by default -- under the system temp dir,
or wherever DUPEHOUND_BENCH_DATA points. Only committed artifacts (results,
methodology, harness code) stay in the repo.
"""
import os
import tempfile


def data_dir():
    base = os.environ.get("DUPEHOUND_BENCH_DATA")
    if not base:
        base = os.path.join(tempfile.gettempdir(), "dupehound-bench")
    os.makedirs(base, exist_ok=True)
    return base
