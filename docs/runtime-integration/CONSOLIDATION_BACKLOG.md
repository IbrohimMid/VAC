# Consolidation Backlog

> Ranked by urgency. Updated after each D-slice batch.
> Last updated: D10.5 (base SHA 3ca3bb57)

## HIGH

- [x] Boundary automation scripts (`scripts/check-dtrack-gates.sh`)
- [x] LOC audit report (`docs/runtime-integration/LOC_AUDIT.md`)
- [ ] Recursive redaction helper (nested objects) — D11 candidate
- [ ] Test-support crate (`vac_shell_test_support`) — reduce FakePaths duplication
- [ ] Docs split: active blueprint vs historical ledger

## MEDIUM

- [ ] `ShellAppProviders` struct — group callback/provider fields in `ShellApp`
- [ ] `ToolUseUiStatus` helper in contracts — single source for severity mapping
- [ ] Projection DTO normalization — align D9/D10 severity mapping to shared helper
- [ ] Duplicate fixture removal across host crate test dirs

## LOW

- [ ] Evaluate host crate merge (host_approval + host_approval_bar)
- [ ] Compact workspace Cargo.toml comments and group members
- [ ] `WORKSPACE_MAP.md` — visual crate dependency map

## DO NOT DO YET

- Merge widget crates
- Merge engine into app
- Replace `vac_tui_runtime` abruptly
- New dispatch capability in D11
- New approval UI feature in D11
