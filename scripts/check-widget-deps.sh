#!/usr/bin/env bash
set -euo pipefail
#
# Strict widget boundary check.
# Widget crates are only allowed to depend on: ratatui, vac_shell_contracts
# (plus transitive std/serde/etc. from contracts, not as direct workspace deps).
#
# Per-crate documented exceptions listed in check_widget calls below.

FAIL=0

check_widget() {
    local pkg="$1"; shift
    local extra_allowed=("$@")

    local toml="crates/${pkg}/Cargo.toml"
    if [[ ! -f "$toml" ]]; then
        echo "MISSING  $pkg (no Cargo.toml)"
        FAIL=1
        return
    fi

    # Extract direct dependency names from [dependencies] section only.
    # Strip .workspace suffix (ratatui.workspace = true → ratatui).
    local deps
    deps=$(awk '/^\[dependencies\]/{found=1; next} /^\[/{found=0} found && /^[a-zA-Z_]/{print $1}' "$toml" \
           | sed 's/\.workspace$//' \
           | sed 's/[[:space:]]*$//' \
           | grep -v '^$' || true)

    local allowed=("ratatui" "vac_shell_contracts" "${extra_allowed[@]}")
    local violations=()

    while IFS= read -r dep; do
        [[ -z "$dep" ]] && continue
        local ok=0
        for a in "${allowed[@]}"; do
            [[ "$dep" == "$a" ]] && { ok=1; break; }
        done
        [[ $ok -eq 0 ]] && violations+=("$dep")
    done <<< "$deps"

    if [[ ${#violations[@]} -gt 0 ]]; then
        echo "VIOLATION  $pkg — unexpected direct deps: ${violations[*]}"
        FAIL=1
    else
        local dep_list
        dep_list=$(echo "$deps" | tr '\n' ' ' | sed 's/[[:space:]]*$//')
        echo "OK  $pkg  [${dep_list}]"
    fi
}

# Pure widget crates: ratatui + vac_shell_contracts only
check_widget vac_shell_palette
check_widget vac_shell_popup
check_widget vac_shell_approval_bar
check_widget vac_shell_approval_detail
check_widget vac_shell_shortcuts
check_widget vac_shell_model_switcher
check_widget vac_shell_session_browser
check_widget vac_shell_diff_view
check_widget vac_shell_activity
check_widget vac_shell_status_bar

# vac_shell_overlay: pure overlay stack (no ratatui render, contracts only)
check_widget vac_shell_overlay

# vac_shell_bridge: action/host trait definitions (contracts only)
check_widget vac_shell_bridge

# vac_shell_plan: serde/serde_json/thiserror for plan JSONL parsing — no engine dep
check_widget vac_shell_plan "serde" "serde_json" "thiserror"

# vac_shell_plan_view: renders plan DTO — allowed dep on vac_shell_plan (same namespace, no engine)
check_widget vac_shell_plan_view "vac_shell_plan"

if [[ $FAIL -ne 0 ]]; then
    echo ""
    echo "WIDGET BOUNDARY FAIL — see violations above"
    exit 1
fi
echo ""
echo "All widget boundaries CLEAN"
