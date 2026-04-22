#!/usr/bin/env python3
"""SLA threshold gate for criterion bench output (M3).

Criterion's `--output-format bencher` emits one line per benchmark:

    test vil_ir::parse_source/small_fixture ... bench:   1234 ns/iter (+/- 56)

This script scans `target/bench-json/*.txt` (the directory passed as argv[1]),
extracts the ns/iter numbers, and compares each against a hard-coded SLA table.
Exits non-zero if any bench is over its threshold.

It is intentionally dumb today — no JSON parsing, no cross-run comparison,
no outlier trimming. Paket C (B2 HNSW) will replace this with a richer
producer-consumer pair once we have stable baselines in git.

Usage:
    python3 scripts/check_bench_slas.py target/bench-json/

Exit codes:
    0 — all benches within SLA (or no relevant benches found, which is ok
        while the harness is being scaffolded)
    1 — at least one bench over threshold
    2 — usage error
"""

from __future__ import annotations

import pathlib
import re
import sys
from typing import Dict, Iterable, List, Optional, Tuple

# SLA table. Keys are `substring-match` against the bench line; value is the
# maximum allowed ns/iter. Keep this conservative — GitHub Actions runners
# have wide variance. Tighten as we collect real baselines.
SLAS_NS_PER_ITER: Dict[str, int] = {
    # vil_ir small fixture: ~200 lines, target < 2ms = 2_000_000 ns.
    "vil_ir::parse_source/small_fixture": 5_000_000,
    # cosine scan with N=100 vectors of dim 384: target < 200 µs. Leave 4x
    # headroom for noisy runners.
    "vil_rag::cosine_scan/100": 800_000,
    # N=1000: target < 2ms; 4x headroom.
    "vil_rag::cosine_scan/1000": 8_000_000,
    # N=10k: target < 25ms; 4x headroom.
    "vil_rag::cosine_scan/10000": 100_000_000,
    # vac cold start: target < 50ms (50_000_000 ns); allow 3x headroom.
    "vac::cold_start/version": 150_000_000,
}

# Matches lines like:
#   test my_group/my_bench ... bench:   1234 ns/iter (+/- 56)
# The group/bench part is the identifier we key the SLA table on.
BENCH_RE = re.compile(
    r"test\s+(?P<name>\S+)\s+\.\.\.\s+bench:\s+(?P<ns>[\d,]+)\s+ns/iter",
)


def parse_bencher_output(text: str) -> Iterable[Tuple[str, int]]:
    for line in text.splitlines():
        match = BENCH_RE.search(line)
        if not match:
            continue
        name = match.group("name")
        ns = int(match.group("ns").replace(",", ""))
        yield name, ns


def find_sla(name: str) -> Optional[int]:
    # Allow the SLA key to be a *substring* of the bench name so we don't have
    # to list every criterion-generated path variant.
    for key, limit in SLAS_NS_PER_ITER.items():
        if key in name:
            return limit
    return None


def main(argv: List[str]) -> int:
    if len(argv) != 2:
        print("usage: check_bench_slas.py <dir-with-bencher-txt>", file=sys.stderr)
        return 2

    bench_dir = pathlib.Path(argv[1])
    if not bench_dir.is_dir():
        print(f"error: {bench_dir} is not a directory", file=sys.stderr)
        return 2

    failures: List[Tuple[str, int, int]] = []
    checked = 0
    seen_files = 0

    for file in sorted(bench_dir.glob("*.txt")):
        seen_files += 1
        print(f"[slas] scanning {file.name}")
        try:
            text = file.read_text(encoding="utf-8", errors="replace")
        except OSError as e:
            print(f"  WARN: cannot read {file}: {e}", file=sys.stderr)
            continue

        for name, ns in parse_bencher_output(text):
            limit = find_sla(name)
            if limit is None:
                print(f"  (no SLA for {name}, {ns} ns/iter — untracked)")
                continue
            checked += 1
            status = "OK" if ns <= limit else "FAIL"
            print(f"  {status:<4} {name}: {ns} ns/iter (limit {limit})")
            if ns > limit:
                failures.append((name, ns, limit))

    if seen_files == 0:
        print("[slas] no bencher output files found — skipping SLA gate")
        return 0

    print(f"[slas] checked {checked} benches across {seen_files} files")
    if failures:
        print("\n[slas] SLA violations:")
        for name, ns, limit in failures:
            over = (ns - limit) / limit * 100.0
            print(f"  - {name}: {ns} ns/iter > {limit} ({over:+.1f}%)")
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
