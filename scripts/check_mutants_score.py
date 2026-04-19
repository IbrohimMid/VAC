#!/usr/bin/env python3
"""Check cargo-mutants JSON output against a minimum score threshold.

Fail-closed: exits non-zero on unreadable data or zero mutants
unless --allow-zero is passed.
"""

import json
import sys


def main():
    allow_zero = "--allow-zero" in sys.argv
    args = [a for a in sys.argv[1:] if a != "--allow-zero"]

    if len(args) < 2:
        print(f"Usage: {sys.argv[0]} <result.json> <threshold> [--allow-zero]")
        sys.exit(2)

    result_path = args[0]
    threshold = int(args[1])

    try:
        with open(result_path) as f:
            data = json.load(f)
    except FileNotFoundError:
        print(f"FAIL: mutants result file not found: {result_path}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"FAIL: mutants result file is not valid JSON: {e}")
        sys.exit(1)

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
        if allow_zero:
            print("WARNING: No mutants found. Skipping score gate (--allow-zero).")
            sys.exit(0)
        else:
            print("FAIL: No mutants found. Gate is fail-closed.")
            print("Pass --allow-zero to explicitly skip when no mutants exist.")
            sys.exit(1)

    score = (caught / total) * 100
    print(f"Mutation score: {score:.1f}% ({caught}/{total})")

    if score < threshold:
        print(f"FAIL: score {score:.1f}% < threshold {threshold}%")
        sys.exit(1)

    print(f"PASS: score {score:.1f}% >= threshold {threshold}%")


if __name__ == "__main__":
    main()
