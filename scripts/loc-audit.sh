#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="docs/runtime-integration/LOC_AUDIT.md"

echo "Generating LOC audit..."

TOTAL_RS=$(find crates -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
TOTAL_MD=$(find docs -name "*.md" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
TEST_RS=$(find crates -name "*.rs" -path "*/tests/*" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
PROD_RS=$((TOTAL_RS - TEST_RS))

VIL_RS=$(find crates -maxdepth 1 -name "vil_*" -type d -exec find {} -name "*.rs" \; \
         | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')

VAC_CORE_RS=$(find \
    crates/vac_core crates/vac_tools crates/vac_trace crates/vac_cli \
    crates/vac_session_engine crates/vac_session_primitives crates/vac_session_control \
    crates/vac_runtime crates/vac_approvals crates/vac_changeset crates/vac_signal \
    crates/vac_tool_core crates/vac_memory crates/vac_mcp_core crates/vac_bridge \
    crates/vac_skill crates/vac_trajectory crates/vac_ingest crates/vac_shell \
    2>/dev/null -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')

SHELL_RS=$(find \
    crates/vac_shell_contracts crates/vac_shell_palette crates/vac_shell_bridge \
    crates/vac_shell_host_surface crates/vac_shell_popup crates/vac_shell_approval_bar \
    crates/vac_shell_host_approval crates/vac_shell_shortcuts crates/vac_shell_host_paths \
    crates/vac_shell_model_switcher crates/vac_shell_host_model crates/vac_shell_composition \
    crates/vac_shell_overlay crates/vac_shell_plan crates/vac_shell_host_plan \
    crates/vac_shell_plan_view crates/vac_shell_session_browser crates/vac_shell_host_sessions \
    crates/vac_shell_activity crates/vac_shell_host_activity crates/vac_shell_diff_view \
    crates/vac_shell_host_diff crates/vac_shell_approval_detail crates/vac_shell_status_bar \
    crates/vac_shell_host_status crates/vac_shell_app crates/vac_shell_entrypoint \
    crates/vac_shell_keymap crates/vac_shell_runtime_loop crates/vac_shell_host_vac_config \
    crates/vac_shell_host_event_projection crates/vac_shell_host_commands \
    crates/vac_shell_host_vac_engine_probe crates/vac_shell_host_vac_command_adapter \
    crates/vac_shell_host_vac_tool_dispatcher crates/vac_shell_host_transcript_projection \
    2>/dev/null -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')

TEST_PROD_RATIO=$(echo "scale=2; $TEST_RS * 100 / $PROD_RS" | bc)

# Top 25 largest crates by RS LOC
TOP_CRATES=$(for d in crates/*/; do
    loc=$(find "$d" -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
    echo "$loc $d"
done | sort -rn | head -25)

# Top 25 largest RS files (skip the "total" summary line from wc -l)
TOP_FILES=$(find crates -name "*.rs" | xargs wc -l 2>/dev/null \
            | sort -rn | awk '!/total/{if(n<25){n++;print}}')

# Top 25 largest MD files
TOP_MD=$(find docs -name "*.md" 2>/dev/null | xargs wc -l 2>/dev/null \
         | sort -rn | awk '!/total/{if(n<25){n++;print}}')

GENERATED_DATE=$(date -u +"%Y-%m-%d %H:%M UTC")
BASE_SHA=$(git rev-parse HEAD)

cat > "$OUT" <<MDEOF
# LOC Audit — VAC Repository

> Generated: ${GENERATED_DATE}
> Base SHA: ${BASE_SHA}

## Summary

| Area | Rust LOC |
|---|---|
| **Total Rust** | ${TOTAL_RS} |
| **Production Rust** | ${PROD_RS} |
| **Test Rust** | ${TEST_RS} |
| **Docs (MD)** | ${TOTAL_MD} |

Test/Prod ratio: ${TEST_PROD_RATIO}%

## LOC per area

| Area | Rust LOC |
|---|---|
| VIL semantic plane (vil_*) | ${VIL_RS} |
| VAC core/runtime/engine | ${VAC_CORE_RS} |
| Shell cockpit + host | ${SHELL_RS} |

## Top 25 crates by Rust LOC

\`\`\`
${TOP_CRATES}
\`\`\`

## Top 25 largest Rust files

\`\`\`
${TOP_FILES}
\`\`\`

## Top 25 largest docs files

\`\`\`
${TOP_MD}
\`\`\`

## Shrink candidates

### Safe
- Test fixture duplication across host crates (FakePaths pattern)
- Historical planning sections in docs → archive/

### Risky
- Inline test helpers in lib.rs files (move to vac_shell_test_support)

### Do not touch
- vac_session_engine core submit path
- vac_shell_contracts DTOs
- D-track host exception crates
MDEOF

echo "LOC audit written to $OUT"
