#!/usr/bin/env sh
set -eu

if [ -n "${FAKE_VIL_MARKER_FILE:-}" ]; then
  printf '%s\n' "$*" >> "${FAKE_VIL_MARKER_FILE}"
fi

printf '[fake-vil] %s\n' "$*"

case "${1:-}" in
  --version|-V|version)
    printf 'vil %s\n' "${FAKE_VIL_VERSION:-1.2.3}"
    exit 0
    ;;
  dev)
    printf '[vil-checkpoint] %s %s\n' "${FAKE_VIL_SESSION_ID:-demo-session}" "${FAKE_VIL_TIMESTAMP:-2026-04-22T00:00:00Z}"
    printf 'vil-dev-stdout\n'
    printf 'vil-dev-stderr\n' >&2
    ;;
  init)
    printf 'vil-init-ok\n'
    ;;
  gen)
    printf 'vil-gen-ok\n'
    ;;
  deploy)
    printf 'vil-deploy-ok\n'
    ;;
esac

exit "${FAKE_VIL_EXIT_CODE:-0}"
