# LOC Audit — VAC Repository

> Generated: 2026-04-26 08:09 UTC
> Base SHA: 3ca3bb577e7159a2a6e6f7944b8f27babff55020

## D10.5 note

Commit `e2b89df2` (D10.5 foundation) was **net-additive (+1181 LOC)**.
It added scripts, docs, `vac_shell_test_support`, recursive redaction helper,
`ShellAppProviders`, and `ToolUseUiStatus`. This was a **LOC reduction foundation**,
not an actual LOC decrease. Actual boilerplate reduction requires adopting the new
helpers — tracked in D10.5-HARDENING.

**Actual reduction target (next pass):**
- Adopt `vac_shell_test_support` in ≥3 test files → target −100 LOC in tests
- Remove duplicate severity mapping in D9/D10 → done in D10.5-HARDENING
- Remove local `write_jsonl` in projection tests → done in D10.5-HARDENING

## Summary

| Area | Rust LOC |
|---|---|
| **Total Rust** | 162351 |
| **Production Rust** | 141448 |
| **Test Rust** | 20903 |
| **Docs (MD)** | 7779 |

Test/Prod ratio: 14.77%

## LOC per area

| Area | Rust LOC |
|---|---|
| VIL semantic plane (vil_*) | 23908 |
| VAC core/runtime/engine | 68401 |
| Shell cockpit + host | 19892 |

## Top 25 crates by Rust LOC

```
50032 crates/vac_tui_runtime/
16360 crates/vac_tools/
13339 crates/vac_cli/
11216 crates/vac_core/
7863 crates/vil_llm/
7255 crates/vac_session_engine/
5709 crates/vil_swarm/
3710 crates/vac_runtime/
2387 crates/vac_bridge/
2305 crates/vac_shell_host_vac_command_adapter/
2063 crates/vil_ir/
1914 crates/vac_skill/
1752 crates/vac_shell_host_model/
1735 crates/vac_memory/
1729 crates/vac_session_primitives/
1426 crates/vac_trajectory/
1405 crates/vac_shell_app/
1370 crates/vil_knowledge/
1234 crates/vac_signal/
1044 crates/vac_trace/
1041 crates/vac_approvals/
998 crates/vac_mcp_core/
993 crates/vil_vwfd/
973 crates/vil_inference/
932 crates/vac_ingest/
```

## Top 25 largest Rust files

```
   1637 crates/vac_core/src/engine.rs
   1493 crates/vil_swarm/src/orchestrator.rs
   1128 crates/vac_shell_host_model/src/lib.rs
   1071 crates/vil_knowledge/src/lib.rs
   1040 crates/vac_approvals/src/lib.rs
   1019 crates/vil_llm/src/router.rs
    957 crates/vac_cli/src/main.rs
    947 crates/vac_cli/src/commands/doctor.rs
    923 crates/vac_tui_runtime/src/system_pulse.rs
    905 crates/vac_core/src/config.rs
    903 crates/vac_core/src/security/secret_detector.rs
    886 crates/vac_tools/src/registry.rs
    872 crates/vac_session_primitives/src/hooks.rs
    866 crates/vac_cli/tests/tui_flows.rs
    846 crates/vil_llm/src/providers/openai.rs
    841 crates/vil_llm/src/providers/openai_compat.rs
    836 crates/vac_shell_host_vac_command_adapter/tests/llm_adapter.rs
    833 crates/vac_tui_runtime/src/action_registry.rs
    741 crates/vac_core/tests/acp_protocol_e2e.rs
    724 crates/vil_llm/src/providers/anthropic.rs
    722 crates/vac_shell_app/src/lib.rs
    721 crates/vil_llm/src/providers/gemini.rs
    721 crates/vac_runtime/src/autopilot.rs
    720 crates/vac_tui_runtime/src/services/helper_block.rs
    718 crates/vac_cli/src/commands/review.rs
```

## Top 25 largest docs files

```
   515 docs/adr/ADR-shellapp-runtime-integration.md
   449 docs/COMPETE_EXECUTION_PLAN.md
   394 docs/ux-spec.md
   367 docs/ROADMAP.md
   310 docs/prd/PRD-CLI.md
   273 docs/ux-gap-analysis.md
   263 docs/PRODUCT_SPEC.md
   239 docs/DONOR_EXTRACTION_MAP.md
   224 docs/COMPETE_BLUEPRINT.md
   214 docs/deployment.md
   212 docs/architecture.md
   187 docs/prd/PRD-TOOLS-MCP.md
   182 docs/prd/PRD-VIL-NATIVE.md
   181 docs/prd/PRD-SECURITY-GOVERNANCE.md
   175 docs/runtime-integration/D-TRACK_STATUS.md
   175 docs/prd/PRD-RUNTIME-SCHEDULER.md
   166 docs/tui/recorder_replay.md
   163 docs/prd/PRD-TUI.md
   163 docs/onboarding.md
   148 docs/runtime_operating_guide.md
   147 docs/prd/PRD-AGENT-SWARM.md
   145 docs/runtime-integration/DOGFOOD_CHECKLIST.md
   138 docs/prd/PRD-SESSION-MANAGEMENT.md
   134 docs/tui/action_matrix.md
   127 docs/runtime-integration/LOC_AUDIT.md
```

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
