#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== D-track validation ==="
echo ""

bash scripts/check-dtrack-gates.sh

echo ""
echo "=== Running D-track test suite ==="
cargo nextest run \
  -p vac_shell_app \
  -p vac_shell_host_event_projection \
  -p vac_shell_host_transcript_projection \
  -p vac_shell_host_approval \
  -p vac_shell_host_sessions \
  -p vac_shell_session_browser \
  -p vac_shell_host_vac_command_adapter \
  -p vac_shell_host_vac_tool_dispatcher \
  -E 'not test(sla_)'

echo ""
echo "D-track validation: PASS"
