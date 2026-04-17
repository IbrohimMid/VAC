#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <vac-binary>" >&2
  exit 2
fi

resolve_target_dir() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
    return 0
  fi

  cargo metadata --format-version 1 --no-deps |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' |
    head -n 1
}

vac_bin="$1"

if [[ "$vac_bin" != /* ]]; then
  target_dir="$(resolve_target_dir)"
  vac_bin="$target_dir/$vac_bin"
fi

if [[ ! -x "$vac_bin" ]]; then
  echo "vac binary is not executable: $vac_bin" >&2
  exit 2
fi

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

pushd "$workdir" >/dev/null

"$vac_bin" --version
"$vac_bin" init --force

test -d .vac
test -f .vac/config.toml

"$vac_bin" doctor
"$vac_bin" runtime status
"$vac_bin" config show

popd >/dev/null
