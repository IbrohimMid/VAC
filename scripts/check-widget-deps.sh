#!/usr/bin/env bash
set -euo pipefail

WIDGETS=(
    vac_shell_palette
    vac_shell_popup
    vac_shell_approval_bar
    vac_shell_approval_detail
    vac_shell_shortcuts
    vac_shell_model_switcher
    vac_shell_session_browser
    vac_shell_diff_view
    vac_shell_plan_view
    vac_shell_activity
    vac_shell_status_bar
)

# Widget crates must only depend on ratatui + vac_shell_contracts (plus std)
FORBIDDEN='vac_session_engine|vac_tools|vil_llm|vac_cli|vac_core|stakpak|stakai'
FAIL=0

for w in "${WIDGETS[@]}"; do
    hits=$(cargo tree -p "$w" -e normal --depth 3 2>/dev/null \
           | grep -E "$FORBIDDEN" || true)
    if [[ -n "$hits" ]]; then
        echo "WIDGET VIOLATION in $w:"
        echo "$hits"
        FAIL=1
    else
        echo "OK  $w"
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "WIDGET BOUNDARY FAIL"
    exit 1
fi
echo "All widget boundaries CLEAN"
