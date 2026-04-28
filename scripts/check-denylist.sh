#!/usr/bin/env bash
set -euo pipefail

# Search for forbidden identifiers in shell cockpit production code.
# Scope: vac_shell_* crates only. vac_tui_runtime and vac_cli are
# donor-extraction zones — their legacy hits are tracked separately.
# Allowed only in docs/test/tripwire/vendor contexts within shell crates.
TERMS=(stakpak SecretManager AutoApproveManager "donor AppState" stakai)
FAIL=0

for term in "${TERMS[@]}"; do
    hits=$(grep -rn --include="*.rs" "$term" \
           crates/vac_shell_contracts \
           crates/vac_shell_app \
           crates/vac_shell_bridge \
           crates/vac_shell_composition \
           crates/vac_shell_overlay \
           crates/vac_shell_runtime_loop \
           crates/vac_shell_keymap \
           crates/vac_shell_entrypoint \
           crates/vac_shell_palette \
           crates/vac_shell_popup \
           crates/vac_shell_approval_bar \
           crates/vac_shell_approval_detail \
           crates/vac_shell_shortcuts \
           crates/vac_shell_model_switcher \
           crates/vac_shell_session_browser \
           crates/vac_shell_diff_view \
           crates/vac_shell_plan_view \
           crates/vac_shell_activity \
           crates/vac_shell_status_bar \
           crates/vac_shell_plan \
           crates/vac_shell_host_approval \
           crates/vac_shell_host_sessions \
           crates/vac_shell_host_activity \
           crates/vac_shell_host_event_projection \
           crates/vac_shell_host_transcript_projection \
           crates/vac_shell_host_vac_command_adapter \
           crates/vac_shell_host_vac_tool_dispatcher \
           crates/vac_shell_host_vac_engine_probe \
           crates/vac_shell_init_checklist \
           crates/vac_shell_host_init \
           crates/vac_shell_host_doctor \
           crates/vac_shell_host_doctor_command \
           crates/vac_shell_host_status_command \
           crates/vac_shell_host_recovery \
           2>/dev/null \
           | grep -v "/tests/" \
           | grep -v "//.*$term" \
           || true)
    if [[ -n "$hits" ]]; then
        echo "DENYLIST hit for '$term':"
        echo "$hits"
        FAIL=1
    else
        echo "OK  '$term' not found in production code"
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "DENYLIST FAIL — see hits above"
    exit 1
fi
echo "Denylist CLEAN"
