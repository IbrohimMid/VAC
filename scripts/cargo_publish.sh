#!/usr/bin/env bash
# NS.7 — leaves-first cargo publish driver.
#
# Publishes workspace crates to crates.io in topological order so
# each publish call sees its dependencies already live. Uses
# `cargo metadata` as the source of truth for the dep graph; no
# hard-coded list to drift.
#
# Usage:
#   CARGO_REGISTRY_TOKEN=... scripts/cargo_publish.sh            # real publish
#   scripts/cargo_publish.sh --dry-run                           # check only
#   scripts/cargo_publish.sh --only vac_tool_core,vil_ir          # subset
#
# Exits non-zero on the first failure; safe to re-run (crates that
# are already at the current version are skipped by cargo with a
# non-fatal warning on subsequent runs).
#
# Audit note: crates.io does not support yanked-then-republish of
# the same version. If a publish halfway through fails, bump the
# workspace version, fix, retry from the failing crate.

set -euo pipefail

DRY_RUN=""
ONLY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN="--dry-run"; shift;;
    --only)    ONLY="$2"; shift 2;;
    *) echo "unknown flag: $1" >&2; exit 2;;
  esac
done

command -v jq >/dev/null 2>&1 || {
  echo "error: jq is required for topo-sort" >&2; exit 2;
}

# Pull every workspace member + its internal (path) deps.
meta=$(cargo metadata --no-deps --format-version=1)

# Members by name. Filter to workspace-owned crates only.
readarray -t MEMBERS < <(
  echo "$meta" | jq -r '.packages[] | select(.source == null) | .name'
)

if [[ -n "$ONLY" ]]; then
  IFS=',' read -r -a OVERRIDE <<<"$ONLY"
  MEMBERS=("${OVERRIDE[@]}")
fi

# Build adjacency: crate → list of its workspace dep names.
declare -A DEPS
for crate in "${MEMBERS[@]}"; do
  readarray -t crate_deps < <(
    echo "$meta" \
      | jq -r --arg c "$crate" '
          .packages[]
          | select(.name == $c)
          | .dependencies[]
          | select(.path != null)
          | .name
        '
  )
  # Join with spaces (arrays in assoc arrays in bash: store as string).
  DEPS[$crate]="${crate_deps[*]:-}"
done

# Kahn's topological sort.
declare -A VISITED
order=()
visit() {
  local crate="$1"
  if [[ -n "${VISITED[$crate]:-}" ]]; then return 0; fi
  VISITED[$crate]=1
  for dep in ${DEPS[$crate]:-}; do
    if [[ -n "${DEPS[$dep]:-}" || " ${MEMBERS[*]} " == *" $dep "* ]]; then
      visit "$dep"
    fi
  done
  order+=("$crate")
}
for crate in "${MEMBERS[@]}"; do visit "$crate"; done

echo "── Publish order (leaves first) ──"
for c in "${order[@]}"; do echo "  $c"; done
echo

if [[ -n "$DRY_RUN" ]]; then
  echo "Running --dry-run across all crates. crates.io won't see any upload."
fi

FAIL_COUNT=0
for crate in "${order[@]}"; do
  echo "── cargo publish -p $crate $DRY_RUN ──"
  if ! cargo publish -p "$crate" --allow-dirty $DRY_RUN; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo "!!  publish failed for $crate" >&2
    if [[ -z "$DRY_RUN" ]]; then
      # Real publish: stop on first failure to avoid a half-
      # published workspace with dangling references.
      exit 1
    fi
  fi
  # crates.io has a short propagation delay after publish; a
  # dependent crate that probes too fast gets "no matching package
  # `foo`". 20s is empirically enough; CI can widen via
  # VAC_PUBLISH_SLEEP.
  if [[ -z "$DRY_RUN" ]]; then
    sleep "${VAC_PUBLISH_SLEEP:-20}"
  fi
done

echo
if [[ "$FAIL_COUNT" -eq 0 ]]; then
  echo "✔ all ${#order[@]} crates processed"
else
  echo "⚠ ${FAIL_COUNT} crates failed (dry-run summary)" >&2
  exit 1
fi
