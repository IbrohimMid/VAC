# D-track Review Checklist

> Use this checklist before marking any D-slice as PASS.

## Before PASS

- [ ] `cargo nextest run` across all affected crates — 0 failures
- [ ] `bash scripts/check-dtrack-gates.sh` — PASS
- [ ] Docs updated: D-TRACK_STATUS.md row added/updated
- [ ] No raw `payload` / `arguments` in UI surfaces (ActivityLog, approval preview)
- [ ] No new host-side ADR exception without entry in D-TRACK_STATUS.md boundary section
- [ ] No new engine dep in `vac_shell_app`, widget crates, or `vac_shell_runtime_loop`
- [ ] BLUEPRINT_CURRENT_STATE.md reflects new baseline SHA (if hardening was needed)

## Common traps

| Trap | Check |
|---|---|
| Inline dep on projection crate in ShellApp | `cargo tree -p vac_shell_app -e normal \| grep vac_session_engine` |
| Binary `ok: bool` losing Warning/Cancelled distinction | grep for `ok: bool` in RuntimeEventView |
| Unbounded/unredacted JSON preview | grep for `to_string_pretty` without cap/filter in approval/bridge |
| Widget crate pulling host-state dep | `bash scripts/check-widget-deps.sh` |
| New exception not ADR-documented | grep D-TRACK_STATUS.md for crate name |

## After PASS

- [ ] Update BLUEPRINT_CURRENT_STATE.md sealed baseline SHA
- [ ] Run `bash scripts/loc-audit.sh` if LOC change > 200 lines
- [ ] Add D11 candidates to CONSOLIDATION_BACKLOG.md if reviewer noted any
