# 2026-Q3 Rebaseline

This audit re-baselines the roadmap against the current `main` branch.

## Summary

- Baseline: current main
- Scope: phases 0 through 6B
- Goal: keep claims tied to evidence, not to intent

## Current evidence index

| Area | Status | Evidence |
| --- | --- | --- |
| Secret detection | landed, mutation evidence still being collected | [crates/vac_core/src/security/secret_detector.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/security/secret_detector.rs), [crates/vac_core/tests/policy_gate.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/tests/policy_gate.rs) |
| Bundle import | landed, fuzz harness added | [crates/vac_core/src/bundle.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/bundle.rs), [crates/vac_core/fuzz/fuzz_targets/bundle_import.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/fuzz/fuzz_targets/bundle_import.rs) |
| Policy gate | wrapper-aware guardrail classifier | [crates/vac_core/src/policy_gate.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/policy_gate.rs) |
| Provider wiring | config-driven router path | [crates/vil_llm/src/router.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vil_llm/src/router.rs), [crates/vac_core/src/engine.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/engine.rs) |

## Rebaseline rule

- Update this file only when a concrete artifact exists.
- If a phase claim cannot be backed by a link, keep the claim out of the audit.
