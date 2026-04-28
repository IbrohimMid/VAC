#!/usr/bin/env bash
set -euo pipefail

DENYLIST='vac_session_engine|vac_tools|vil_llm|vac_cli|stakpak|stakai'
FAIL=0

check() {
    local pkg="$1"; local depth="$2"
    local hits
    hits=$(cargo tree -p "$pkg" -e normal --depth "$depth" 2>/dev/null \
           | grep -E "$DENYLIST" || true)
    if [[ -n "$hits" ]]; then
        echo "BOUNDARY VIOLATION in $pkg:"
        echo "$hits"
        FAIL=1
    else
        echo "OK  $pkg"
    fi
}

check vac_shell_app             4
check vac_shell_session_browser 3
check vac_shell_runtime_loop    4
check vac_shell_init_checklist  3
check vac_shell_keymap          4
check vac_shell_activity       4

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "BOUNDARY FAIL — see violations above"
    exit 1
fi
echo "All shell boundaries CLEAN"
