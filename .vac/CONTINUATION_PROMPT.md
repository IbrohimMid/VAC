# Prompt untuk Sesi Lanjutan: TUI Post-Batch-5 Hardening

## Context

Repo: `/home/emp/Documents/VAC/vastar-agentic-cli`  
Baseline: commit `98e4d4f` (2026-04-12 22:50 WIB)  
Status: **Phase 6-9 complete, Phase 10 partial**

Sesi sebelumnya menyelesaikan implementasi bertahap TUI hardening pasca Batch 5 (Batch 4-5 sudah landed). Total 11 commits dibuat untuk cleanup, policy hardening, revert engine-native, session ergonomics, dan testability awal.

---

## Status Implementasi

### ✅ Selesai (11 commits)

**B1: Audit** (< 5 menit)
- Inventory orphan modules, hardcoded tool classifications, boundary violations

**Phase 9: Cleanup** (1 commit: `21b91e1`)
- Hapus orphan `lanes.rs`
- Boundary sudah clean (Task 9.2-9.3 skipped)

**Phase 6: Policy Hardening** (2 commits: `be356d1`, `2ccace3`)
- Extract tool classification ke `services/tool_policy.rs` (single source of truth)
- `TuiPolicyEngine` delegates ke `tool_policy::needs_approval()`
- 5 unit tests pass

**Phase 7: Revert Engine-Native** (3 commits: `66cc7ab`, `22c3ae2`, `685f1ac`)
- Implement `SnapshotManifest` API di `vac_core/src/snapshot.rs`
- Wire TUI revert ke manifest API (no more manual `.bak` scan)
- Add snapshot preview to revert confirm modal
- 2 unit tests pass

**Phase 8: Session/History Ergonomics** (1 commit: `dbfcd03`)
- Add TUI state fields to `SessionMetadata` (active_tab, history_selection, last_focus)
- `save_tui_state()` method (restore wiring TODO)
- Document `DetailMode` state machine transitions
- Document focus cycle behavior
- Task 8.2 (filter) marked TODO

**Phase 10: Testability** (1 commit: `98e4d4f`)
- 3 unit tests for `HistoryState` (select_prev/next, total_visual_rows)
- Task 10.2-10.4 marked TODO

---

### ⏳ TODO (Sisa Phase 10)

**Task 10.2:** Unit tests untuk `services/detail.rs`
- Test `DetailMode::is_some()`
- Test `render_detail_content()` wrapping
- Test `RevertConfirm` preview

**Task 10.3:** Unit tests untuk `services/telemetry.rs`
- Test `route_update()` routing logic
- Test read tool → Commands lane
- Test write tool → Reading lane
- Test error → Thinking lane

**Task 10.4:** Integration tests untuk TUI state flow
- File: `crates/vac_cli/tests/tui_flows.rs` (new)
- Test: Enter history → detail
- Test: R → revert confirm
- Test: Esc → close
- Test: Tab → focus cycle
- Test: approval modal

**Target coverage:** 70-80% untuk services, semua shortcut kritis

---

### ⏳ Future Work (Optional)

- **Task 8.1 wiring:** Connect `save_tui_state()` to session save on exit
- **Task 8.2 impl:** History filter/search dengan `/` key
- **Task 10.2-10.4 extend:** More comprehensive test coverage

---

## Verification

```bash
# Build check
cargo check --quiet

# Run existing tests
cargo test -p vac_core snapshot
cargo test -p vac_cli history
cargo test -p vac_cli tool_policy

# Manual TUI test
cargo run --bin vac -- interactive
# Test: Tab cycle, Enter history, R revert, Esc close, mouse click
```

---

## Prompt untuk Sesi Baru

```
Lanjutkan implementasi TUI Post-Batch-5 Hardening dari commit 98e4d4f.

**Status:**
- Phase 6-9: ✅ Complete
- Phase 10: ⚠️ Partial (Task 10.1 done, 10.2-10.4 TODO)

**Target sesi ini:**
Selesaikan Phase 10 (Task 10.2-10.4) untuk testability complete.

**Task 10.2:** Unit tests untuk `services/detail.rs`
- Test `DetailMode::is_some()` untuk semua variants
- Test `render_detail_content()` wrapping untuk long text
- Test `RevertConfirm` preview tampilkan file list
- Target: `cargo test -p vac_cli detail` pass

**Task 10.3:** Unit tests untuk `services/telemetry.rs`
- Test `route_update()` untuk berbagai `RuntimeUpdate` variants
- Test routing: read tool → Commands, write tool → Reading, error → Thinking
- Target: `cargo test -p vac_cli telemetry` pass

**Task 10.4:** Integration tests untuk TUI state flow
- File baru: `crates/vac_cli/tests/tui_flows.rs`
- Test flow: Enter history → detail, R → revert confirm, Esc → close, Tab → focus cycle
- Test approval modal: pending_approval state
- Target: `cargo test -p vac_cli tui_flows` pass

**Constraints:**
- Minimal code only (sesuai implicit instruction)
- vac_cli adalah binary crate, tests harus di module atau integration tests
- Gunakan actual struct definitions (TaskHistoryEntry dari vac_core::engine, TaskStatus dari vac_core::task)
- Build harus tetap clean setelah setiap task

**Deliverable:**
- 3 commits (1 per task)
- Semua tests pass
- Coverage target: 70-80% untuk services

Mulai dari Task 10.2.
```

---

## File Referensi

**Services yang perlu di-test:**
- `crates/vac_cli/src/tui/services/detail.rs`
- `crates/vac_cli/src/tui/services/telemetry.rs`

**State structs:**
- `crates/vac_cli/src/tui/app.rs` (TuiApp, SessionTab, FocusPane, PendingApproval)
- `crates/vac_cli/src/tui/services/detail.rs` (DetailMode)

**Event handling:**
- `crates/vac_cli/src/tui/event_loop.rs` (handle_key, handle_task_event)

---

## Catatan Penting

1. **vac_cli binary crate:** Tests harus pakai `#[cfg(test)] mod tests` di dalam module atau integration tests di `tests/`
2. **TaskHistoryEntry:** Import dari `vac_core::engine`, bukan `vac_core::task`
3. **TaskStatus:** Import dari `vac_core::task` (enum: Completed, Failed, Running)
4. **Build clean:** Hanya pre-existing warnings di `bash.rs` yang boleh ada

---

## Commit Message Format

```
test(tui): add unit tests for services/detail.rs

- Test DetailMode::is_some() for all variants
- Test render_detail_content() wrapping
- Test RevertConfirm preview file list
- N tests pass

Task 10.2 complete
```

---

Gunakan prompt di atas untuk melanjutkan di sesi baru. Semua context sudah lengkap untuk pickup dari commit `98e4d4f`.
