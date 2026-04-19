# VAC TUI Hardening Closure Plan

**Status**: Closed implementation record; remote evidence pending  
**Last updated**: 2026-04-18  
**Use with**: [docs/tui_hardening_masterplan.md](./tui_hardening_masterplan.md)

## Purpose

Dokumen ini adalah catatan closure untuk menutup gap yang masih tersisa setelah hardening besar awal landed. Posisi yang jujur saat ini:

- **major hardening landed**
- **phase 1-12 implementation closed**
- **remote internal deployment evidence pending**

Plan ini tidak mengulang klaim checklist lama. Fokusnya adalah **closure nyata**, **perbaikan kontrak produk**, dan **evidence yang bisa diaudit** untuk seluruh phase `1-12`.

## Delivery Rules

1. Tidak ada fitur TUI baru sampai Phase 1-4 dan blocker trust selesai.
2. Semua phase hanya boleh ditandai selesai jika acceptance gate teknis lulus, bukan karena struktur file sudah berubah.
3. Semua perubahan yang menyentuh trust, release, approval, shell, telemetry, atau startup harus punya test atau evidence baru.
4. Semua pseudo-command, placeholder crate, dan synthetic evidence harus diperlakukan sebagai debt aktif.

## Overall Status

| Phase | Area | Current status | Closure target |
| --- | --- | --- | --- |
| 1 | TUI kernel refactor | Closed | Remove giant-router debt for real |
| 2 | Unified command system | Closed | Eliminate pseudo-operator commands |
| 3 | Startup hydration and truthful boot | Closed | Hydrate first frame from real runtime state |
| 4 | I/O reliability and backpressure | Closed | Add full behavioral coverage |
| 5 | Approval subsystem extraction | Closed | Finish policy/state ownership split |
| 6 | Shell runtime hardening | Closed | Tighten lifecycle and UX determinism |
| 7 | Runtime telemetry surfacing | Closed | Surface full operator-visible runtime contract |
| 8 | Switcher hardening | Closed | Request-on-open and real data parity |
| 9 | Changeset/review/editor unification | Closed | Complete workflow and UX parity |
| 10 | Workspace split | Closed | Remove placeholder crates and finish migration |
| 11 | Testing and evidence hardening | Closed | Reinstate TUI behavior tests and closure gates |
| 12 | Product truthfulness polish | Closed | Rewrite UX copy and empty states from actual state |

## Phase Plan

### Phase 1 - TUI Kernel Refactor Closure

**Problem**

`controller.rs` is no longer the old monolith, but complexity has mostly moved into `handlers/input.rs`. That means precedence risk still exists, only in a new location.

**Implementation**

- Split `vac_tui_runtime/src/handlers/input.rs` into:
  - `input_core.rs`
  - `input_popup.rs`
  - `input_shell.rs`
  - `input_commands.rs`
  - `input_editor.rs`
- Keep `controller.rs` and `handlers/input.rs` as orchestration entrypoints only.
- Move popup interception rules into one explicit dispatcher function.
- Introduce a small precedence matrix doc comment near the dispatch layer.

**Targets**

- `crates/vac_tui_runtime/src/controller.rs`
- `crates/vac_tui_runtime/src/handlers/input.rs`
- new split handler modules under `crates/vac_tui_runtime/src/handlers/`

**Acceptance gates**

- `handlers/input.rs` no longer contains the majority of key routing logic.
- Popup interception order is testable in isolation.
- At least one regression test exists per input context: normal, popup, shell, review, ask-user.

### Phase 2 - Unified Command System Closure

**Problem**

Command metadata is more centralized than before, but operator-facing command semantics are still mixed. `Passthrough` commands with `wired: false` are still visible in operator surfaces.

**Implementation**

- Add explicit `CommandSurface`:
  - `OperatorAction`
  - `AgentPrompt`
  - `Template`
  - `Hidden`
- Add real `execute_command()` in `services/commands.rs`.
- Make slash input, helper dropdown, and command palette call the same executor.
- Remove pseudo operator-grade visibility for:
  - `/vil`
  - `/swarm`
  - `/rulebook`
  - `/resume`
  - `/help`
- Keep passthrough commands available only in contexts that are honest about what they are.

**Targets**

- `crates/vac_tui_runtime/src/services/commands.rs`
- `crates/vac_tui_runtime/src/services/helper_block.rs`
- `crates/vac_tui_runtime/src/services/shortcuts_popup.rs`
- `crates/vac_tui_runtime/src/handlers/input.rs`

**Acceptance gates**

- No visible operator command lacks an explicit product contract.
- Typed slash, dropdown, and palette produce the same action for the same command.
- Tests cover no-phantom-command behavior and parity across all three surfaces.

### Phase 3 - Startup Hydration and Truthful Boot Closure

**Problem**

`StartupSnapshot` exists, but hydration is still shallow. First frame truthfulness is improved, not completed.

**Implementation**

- Expand startup hydration to include:
  - installed version
  - latest version if reachable
  - current provider/auth status
  - active model and default model
  - active profile
  - selected rulebooks
  - MCP server status
  - session count
  - pending approvals count
  - runtime queue summary
- Make startup state source-driven from runtime/bootstrap calls, not defaults alone.
- Ensure operator pane and activity pane render hydration status, not generic emptiness.

**Targets**

- `crates/vac_tui_runtime/src/app/types.rs`
- `crates/vac_tui_runtime/src/event_loop.rs`
- `crates/vac_tui_runtime/src/runner.rs`
- `crates/vac_tui_runtime/src/services/statusline.rs`
- `crates/vac_tui_runtime/src/services/helper_block.rs`
- `crates/vac_tui_runtime/src/view.rs`

**Acceptance gates**

- No `vunknown` or `Model: none` style ambiguity on first frame.
- If no model is active, UI says `no active model selected` and shows recovery path.
- Startup snapshot fields are populated from actual bootstrap logic, not mostly defaults.

### Phase 4 - I/O Reliability and Backpressure Closure

**Problem**

Queueing exists, but coverage and edge-case guarantees are still light.

**Implementation**

- Add bounded queue policy and explicit metrics for:
  - queued user messages
  - dropped messages
  - merged messages
  - flush retries
- Add merge semantics for rapid slash submits vs normal text submits.
- Add explicit recovery UX when channel flush repeatedly fails.

**Targets**

- `crates/vac_tui_runtime/src/app/types.rs`
- `crates/vac_tui_runtime/src/update.rs`
- `crates/vac_tui_runtime/src/handlers/input.rs`
- `docs/RUNTIME_QUEUE_BOUNDARY.md`

**Acceptance gates**

- No duplicate send under retry.
- No silent drop under busy/loading state.
- Behavioral tests cover flush, retry, merge, and full-queue scenarios.

### Phase 5 - Approval Subsystem Closure

**Problem**

The crate split is real, but closure still depends on making approval policy ownership fully domain-centric and keeping UI thin.

**Implementation**

- Move any remaining approval-specific selection or batch ordering rules out of TUI state if still duplicated.
- Add explicit batch ordering and stale-approval rejection tests in `vac_approvals`.
- Make approval UI read from approval-domain projections rather than rebuilding logic locally.

**Targets**

- `crates/vac_approvals/src/`
- `crates/vac_tui_runtime/src/handlers/approval.rs`
- `crates/vac_tui_runtime/src/app/types.rs`
- `crates/vac_core/src/engine.rs`

**Acceptance gates**

- Approval ordering is deterministic and tested in domain crate.
- UI contains no policy parsing logic.
- Approve/reject current/all and reject-with-reason are stable under repeated operations.

### Phase 6 - Shell Runtime Hardening Closure

**Problem**

Shell runtime is materially landed, but operator behavior still needs stricter lifecycle guarantees and better UX surfacing.

**Implementation**

- Formalize lifecycle states:
  - `Starting`
  - `PromptReady`
  - `Running`
  - `WaitingInput`
  - `Backgrounded`
  - `Exited`
  - `Killed`
  - `Errored`
- Surface transition reasons to TUI.
- Add session-switch cleanup and shell-focus restoration tests.
- Add bounded-output and stall-recovery behavior to operator UI.

**Targets**

- `crates/vac_shell/src/lib.rs`
- `crates/vac_tui_runtime/src/handlers/shell.rs`
- `crates/vac_tui_runtime/src/app/types.rs`
- `crates/vac_cli/tests/shell_lifecycle.rs`

**Acceptance gates**

- No orphan shell after session switch.
- Background/focus/kill lifecycle is deterministic.
- Operator can see why a shell exited or stalled.

### Phase 7 - Runtime Telemetry Surfacing Closure

**Problem**

Telemetry is better surfaced, but not yet a complete operator contract.

**Implementation**

- Make runtime panes explicitly show:
  - active model
  - provider/auth state
  - validation result summary
  - LSP diagnostic counts
  - current tool timeline
  - cancellation reason
  - retry/backoff status
  - task graph position
- Normalize telemetry taxonomy across activity pane and runtime/workbench tabs.

**Targets**

- `crates/vac_tui_runtime/src/app/events.rs`
- `crates/vac_tui_runtime/src/update.rs`
- `crates/vac_tui_runtime/src/runner.rs`
- `crates/vac_tui_runtime/src/services/statusline.rs`
- `crates/vac_tui_runtime/src/services/vil_workbench.rs`
- `crates/vac_tui_runtime/src/view.rs`

**Acceptance gates**

- Operator can answer major runtime questions from UI alone.
- No major runtime signal is dropped into `_ => {}` without intent.
- Telemetry surfaces are consistent between statusline, activity, and workbench.

### Phase 8 - Switcher Hardening Closure

**Problem**

Switcher lifecycle improved, but data loading and state truthfulness are not fully aligned across model/profile/rulebook.

**Implementation**

- Make each switcher implement the same lifecycle contract:
  - `open`
  - `close`
  - `update_filter`
  - `select_next`
  - `select_prev`
  - `submit_selected`
- Ensure data source is real:
  - model list from provider state
  - profile list from real profile registry
  - rulebook list from actual rulebook loader
- Block switcher open if a higher-priority modal is active.

**Targets**

- `crates/vac_tui_runtime/src/handlers/model_switcher.rs`
- `crates/vac_tui_runtime/src/handlers/profile_switcher.rs`
- `crates/vac_tui_runtime/src/handlers/rulebook_switcher.rs`
- `crates/vac_tui_runtime/src/app/types.rs`

**Acceptance gates**

- Keyboard-only flow works across all switchers.
- Active item is preselected consistently.
- Empty switcher state is honest and distinguishable from loading state.

### Phase 9 - Changeset, Review, and Editor Closure

**Problem**

`vac_changeset` is real, but workflow parity across all review surfaces still needs closure.

**Implementation**

- Ensure side panel, review tab, file changes popup, and editor-open actions all read from the same store.
- Add filtered revert actions:
  - revert selected
  - revert filtered
  - revert all
- Expose snapshot availability and revert failure reasons uniformly.

**Targets**

- `crates/vac_changeset/src/lib.rs`
- `crates/vac_tui_runtime/src/handlers/changeset.rs`
- `crates/vac_tui_runtime/src/handlers/review.rs`
- `crates/vac_tui_runtime/src/services/file_changes_popup.rs`
- `crates/vac_tui_runtime/src/services/side_panel.rs`

**Acceptance gates**

- Counts and selection state are consistent across all review surfaces.
- Revert operations reflect immediately in all surfaces.
- Failure/reverted states are operator-visible and tested.

### Phase 10 - Workspace Split Closure

**Closure record**

Phase 10 is closed. `vac_session_control` now carries real domain logic for:

- session snapshot model
- session save/load helpers
- cleanup/reset rules
- migration/version boundaries

The extracted crates are used by real codepaths:

- `vac_tui_runtime`
- `vac_shell`
- `vac_changeset`
- `vac_approvals`
- `vac_session_control`

Temporary migration scripts remain only if they still serve active repo maintenance.

**Closure gates satisfied**

- No placeholder crate remains in the workspace.
- Each extracted crate is used by a real codepath.
- `vac_cli` remains the CLI entrypoint, not a control-plane dump.

### Phase 11 - Testing and Evidence Hardening Closure

**Problem**

Tests compile and many pass, but the most important TUI behavior coverage is still missing or disabled.

**Implementation**

- Rebuild `tui_flows.rs` as real integration tests.
- Add or restore tests for:
  - command parity
  - popup interception
  - startup snapshot hydration
  - switcher request-on-open
  - buffered message queue
  - approval batch ordering
  - shell session lifecycle
  - session switch cleanup
  - telemetry propagation
- Add CI-visible test grouping or naming so closure can be tracked explicitly.

**Targets**

- `crates/vac_cli/tests/tui_flows.rs`
- `crates/vac_cli/tests/command_surface_tests.rs`
- `crates/vac_cli/tests/shell_lifecycle.rs`
- `crates/vac_tui_runtime/src/contracts_test.rs`

**Acceptance gates**

- No core TUI behavior test file is left empty or TODO-only.
- Tests cover the hardening contract, not just JSON CLI output.
- CI can prove the core operator flows are protected.

### Phase 12 - Product Truthfulness Polish Closure

**Problem**

Surface copy and empty states improved, but product truthfulness is not fully normalized.

**Implementation**

- Rewrite:
  - banner copy
  - empty-state copy
  - activity taxonomy labels
  - operator pane wording
  - shell/approval footers
- Make every empty state explicit:
  - loading
  - not configured
  - unavailable
  - no active selection
  - no recent activity
- Review all startup/operator text against actual product state.

**Targets**

- `crates/vac_tui_runtime/src/services/banner.rs`
- `crates/vac_tui_runtime/src/services/helper_block.rs`
- `crates/vac_tui_runtime/src/services/statusline.rs`
- `crates/vac_tui_runtime/src/services/side_panel.rs`
- `crates/vac_tui_runtime/src/view.rs`

**Acceptance gates**

- No deceptive or overconfident copy remains.
- Empty states map to real underlying state categories.
- UX wording matches actual implementation and capability exposure.

## Parallel Closure Track

### Trust and Release Hardening

Ini berjalan paralel terhadap Phase 1-12, tetapi harus ditutup sebelum repo bisa diklaim mendekati production-grade.

#### T1 - Mutation gate fail-closed

- Closed: `scripts/check_mutants_score.py` fails when JSON is unreadable.
- Closed: it fails when `total_mutants == 0` unless explicitly whitelisted.
- Workflow output artifact upload remains, but no gate bypass remains.

#### T2 - Release trust chain closure

- Closed: installer checksum expectation matches the actual published checksum manifest.
- Closed: installer consumes `SHA256SUMS.txt`.
- Public verification materials are still tracked separately if signing is claimed externally.
- Closed: placeholder repo references were removed from `scripts/install.sh`.

#### T3 - Operability enforcement closure

- Closed: `memory_cap_bytes` and `disk_quota_bytes` are enforced in runtime paths that matter.
- Closed: quota application has coverage where feasible.
- Closed: failure reporting is surfaced to the operator.

#### T4 - Trace redaction end-to-end

- Closed: recorder, export, crash dump, and panic paths share one redaction contract.
- Closed: raw payload leak paths are removed.
- Closed: end-to-end tests prove redaction under failure conditions.

#### T5 - Evidence gate closure

- Closed for repo-local verification: the evidence index now points at a real audit bundle instead of fabricated CI IDs.
- Closed for release smoke verification: the release binary has a recorded SHA256 and passed the smoke contract.
- Still pending for true internal deployment evidence if the project wants to claim that separately.
- Remote GitHub audit found no release or deployment evidence; the claim remains pending.
- `STABILITY_LOG.md` remains an index, not the proof itself.

## Execution Order

### Blocker-first order

1. Phase 10 closure for `vac_session_control`
2. Trust T1 mutation gate
3. Trust T2 release trust chain
4. Phase 2 command contract cleanup
5. Phase 3 startup hydration closure
6. Phase 11 test closure

### Main hardening order

1. Phase 1
2. Phase 2
3. Phase 3
4. Phase 4
5. Phase 5
6. Phase 6
7. Phase 7
8. Phase 8
9. Phase 9
10. Phase 10
11. Phase 11
12. Phase 12

## Completion Definition

Plan ini hanya boleh dinyatakan selesai bila:

- semua placeholder crate sudah hilang
- pseudo-command operator surface sudah dibersihkan
- startup state benar-benar hydrated dari runtime/bootstrap nyata
- behavioral TUI tests kembali hidup
- mutation gate benar-benar fail-closed
- installer dan release artifacts sinkron
- evidence log berisi artefak nyata yang bisa diaudit

Sebelum rilis closure final, trust-track evidence masih tetap diperlakukan sebagai bukti terpisah.

> **major hardening landed, phase 1-12 implementation closed; remote evidence pending**
