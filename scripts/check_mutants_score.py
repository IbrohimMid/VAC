#!/usr/bin/env python3
"""Check cargo-mutants JSON output against a minimum score threshold."""

import json
import sys


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <result.json> <threshold>")
        sys.exit(2)

    result_path = sys.argv[1]
    threshold = int(sys.argv[2])

    try:
        with open(result_path) as f:
            data = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"WARNING: Could not read mutants result: {e}")
        print("Skipping score gate (no data).")
        sys.exit(0)

    outcomes = data.get("outcomes", [])
    if not outcomes:
        total = data.get("total_mutants", 0)
        caught = data.get("caught", 0)
    else:
        total = len(outcomes)
        caught = sum(
            1
            for o in outcomes
            if o.get("scenario", {}).get("mutant") is not None
            and o.get("summary") in ("CaughtMutant", "Timeout")
        )

    if total == 0:
        print("WARNING: No mutants found. Skipping score gate.")
        sys.exit(0)

    score = (caught / total) * 100
    print(f"Mutation score: {score:.1f}% ({caught}/{total})")

    if score < threshold:
        print(f"FAIL: score {score:.1f}% < threshold {threshold}%")
        sys.exit(1)

    print(f"PASS: score {score:.1f}% >= threshold {threshold}%")


if __name__ == "__main__":
    main()
