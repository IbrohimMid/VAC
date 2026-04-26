#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== D-track boundary gate: shell boundaries ==="
bash scripts/check-shell-boundaries.sh

echo ""
echo "=== D-track boundary gate: widget deps ==="
bash scripts/check-widget-deps.sh

echo ""
echo "=== D-track boundary gate: denylist ==="
bash scripts/check-denylist.sh

echo ""
echo "D-track boundary gates: PASS"
