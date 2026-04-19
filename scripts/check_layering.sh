#!/usr/bin/env bash
# Enforces the architectural invariant that vil_* crates do not depend on
# vac_* crates. This preserves the "VIL is a reusable engine; VAC is its
# CLI/runtime host" layering contract.
#
# Allowed exceptions:
#   - vil_swarm → vac_tools   (approval + tool routing bridge)
#
# Exit 0 on success; exit 1 on any forbidden edge with a human-readable report.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null; then
    echo "error: cargo not found" >&2
    exit 2
fi

metadata=$(cargo metadata --format-version 1 --no-deps)

violations=$(python3 - <<'PY' "$metadata"
import json, sys
meta = json.loads(sys.argv[1])
allowed = {("vil_swarm", "vac_tools")}
bad = []
for pkg in meta["packages"]:
    name = pkg["name"]
    if not name.startswith("vil_"):
        continue
    for dep in pkg.get("dependencies", []):
        dep_name = dep["name"]
        if dep_name.startswith("vac_") and (name, dep_name) not in allowed:
            bad.append(f"{name} -> {dep_name}")
for v in bad:
    print(v)
PY
)

if [ -n "$violations" ]; then
    echo "Layering violations detected:" >&2
    echo "$violations" | sed 's/^/  /' >&2
    echo "" >&2
    echo "vil_* crates must not depend on vac_* crates (except allowed bridges)." >&2
    exit 1
fi

echo "Layering check passed: no forbidden vil_* -> vac_* edges."
