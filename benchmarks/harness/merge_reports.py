#!/usr/bin/env python3
"""Merge two scaling-run JSONs (e.g. Claude half + OpenAI half) into one report.

dupehound rows are deterministic and taken from the first file; agent rows from
both are concatenated. Produces a single combined .md (with the per-type
breakdown, DNF marking, etc.) and .json.

Usage: python3 merge_reports.py claude.json gpt.json out_basepath
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run_scaling  # noqa: E402


def main():
    a = json.load(open(sys.argv[1]))
    b = json.load(open(sys.argv[2]))
    out = os.path.abspath(sys.argv[3])

    meta = dict(a["meta"])
    meta["models"] = list(a["meta"].get("models", [])) + list(b["meta"].get("models", []))
    meta["notes"] = (a["meta"].get("notes") or []) + (b["meta"].get("notes") or [])
    meta["spent"] = (a["meta"].get("spent") or 0.0) + (b["meta"].get("spent") or 0.0)

    dh_rows = a["dupehound"]
    agent_rows = a["agents"] + b["agents"]

    with open(out + ".json", "w") as f:
        json.dump({"meta": meta, "dupehound": dh_rows, "agents": agent_rows}, f, indent=2)
    run_scaling.write_markdown(out + ".md", meta, dh_rows, agent_rows)
    print("merged -> %s.md" % out)


if __name__ == "__main__":
    main()
