# VAC TUI Parity — Remaining Work Plan (Wave 2.5 → 3 → 4)

> Diturunkan dari `VAC_TUI_PARITY_PLAN_WAVE_25_TO_4.md` pada 2026-04-21 setelah PR-T12 service-layer landed.
> **Status map diperbarui 2026-04-22** setelah audit HEAD `7d47b55`.
> Fokus dokumen ini: **apa yang belum selesai + cara kerjakan + kriteria audit.**
> Local coding agent adalah eksekutor; dokumen ini adalah directive + test plan + review rubric.

---

## 1. Status map (evidence-based, 2026-04-22)

| PR | Status | Evidence |
|---|---|---|
| W25-1 Theme sweep | ✅ **complete** | raw `Color::` outside theme = 0 (audited 2026-04-22) |
| W25-2 PR-T5 TOML + notify | ✅ **complete** | `services/theme_loader.rs` + `watch_theme_emits_within_500ms` hijau |
| W25-3 Split shortcuts_popup | ✅ **complete** | `services/shortcuts_popup/catalog.rs` ada |
| W25-4 Split input_popup | ✅ **complete** | `handlers/input_popup/misc_overlays.rs`; max file < 600 LOC |
| W25-5 Split markdown_renderer | ✅ **complete** | `services/markdown_renderer/{renderer,layout,...}`; max file < 600 LOC |
| W25-6 Split view.rs | ✅ **complete** | `view/{mod,overlays,operator,...}`; max file < 600 LOC |
| W25-7 Split event_loop_tests | ✅ **complete** | `event_loop_tests/{session,approval,shell,runtime}.rs` ada |
| W25-8 Split app/types.rs | ✅ **complete** | `app/types/mod.rs` ada sebagai folder |
| W25-9 Split runner.rs + async | ✅ **complete** | `runner.rs` = 427 LOC; `block_in_place` = 0 |
| §8 PR-2 vil_expr crate | ✅ **complete** | Commit `28e461f` |
| §8 PR-3 VacConfig::vil | ✅ **complete** | Commit `426a063` (subsumed inline — ADR-0001) |
| PR-T11 VWFD inspector | ✅ **complete** | `VwfdInspectorState` wired, service + handlers present |
| PR-T12 vil-expr validator | ✅ **complete** | Service-layer + 6 tests; UI hook (T12.1) → Task-4 |
| PR-T12.1 UI wiring | ✅ **complete** | `input_changed_updates_vil_expr_lint_state`, `tick_emits_redraw_request_after_debounce`, `alt_h_on_identifier_opens_popup` — all 3 PASS in `handlers/workspace_input.rs` |
| §8 PR-5b vwfd_parity_pass | ✅ **complete** | `vwfd_parity_pass` wired |
| §8 PR-6 VWFD semantic diff | ✅ **complete** | `vac_changeset::formats::vwfd` present |
| PR-T13 VIL-aware diff overlay | ✅ **complete** | `vwfd_diff_render` delegation in `services/review.rs` |
| PR-T14 Background vil dev | ⚠️ **materially-advanced** | Substrate + log panel; runner event routing → Task-6, Task-7 |
| PR-T15 Inline diagnostics | ✅ **complete** | `diagnostics_consumers` wired |
| PR-T16 Mouse dispatch | ⚠️ **partial** | Tab/tray/banner wired; surface gap → Task-5 |
| PR-T17 Kitty image | ⚠️ **partial** | DCS probe + cache; LRU + PTY e2e → Task-8, Task-9 |
| PR-T18 Recorder + Replay | ✅ **complete** | JSONL writer/reader + event-loop tap + CLI flag |
| PR-T19 Keybindings | ✅ **complete** | Loader + watcher + runtime wiring |

---

## 2. Urutan eksekusi yang disarankan

```
# Quick wins dulu (unblock T13 dan self-contained):
PR-T12.1 (UI wiring T12)   ──┐
                             │
W25-7 (split event_loop)     │
W25-9 (runner split + async) │
W25-1 (theme sweep)   ───────┤
                             ↓
                        PR-T11 (VWFD inspector)
                             ↓
                   §8 PR-5b  +  §8 PR-6
                             ↓
                        PR-T13 (diff overlay)
                             ↓
                        PR-T14 (vil dev)
                             ↓
                        Wave 4 (paralel)
```

Alasan: T12.1 kecil, bisa dikerjakan paralel dengan W25 sisanya; W25-1 paling besar tapi tidak memblok Wave 3 kecuali exit-gate diaktifkan.

---

## 3. PR-T12.1 — UI wiring vil-expr validator (follow-up T12)

**Scope:** tutup 3 item DEFERRED dari PR-T12.

**Files:**
- `crates/vac_tui_runtime/src/handlers/input.rs` (modify)
- `crates/vac_tui_runtime/src/event_loop.rs` atau path tick-driver (modify — tambah pemanggil `.tick()`)
- `crates/vac_tui_runtime/src/view/input.rs` atau `view/overlays.rs` (modify — render issues)
- `crates/vac_tui_runtime/src/handlers/input.rs` (tambah Alt+H handler)

**Perubahan:**
1. Setiap `InputChanged` (atau setara) panggil `state.vil_expr_lint.on_input_changed(&state.composer.text, Instant::now())`.
2. Di tick loop (per frame atau per event), panggil `state.vil_expr_lint.tick(&SymbolTable::new(), Instant::now())` — jika return `true`, minta repaint.
3. Render issues di bawah composer: iterasi `state.vil_expr_lint.issues()` → `Line` dengan `StyleKey::ValidationError` / `ValidationWarning` / `ValidationOk`. Tampilkan hanya kalau `!issues.is_empty()` atau payload `vil-expr:` aktif (kosongkan kalau user hapus prefix).
4. Alt+H keybind: kalau cursor di atas identifier dalam payload → popup berisi `format!("{}: {:?}", ident, inferred_type)`. Untuk placeholder, tampilkan pesan `"type inference coming soon (PR-T12 stub)"` — grammar belum final.

**Tests (wajib, di file yang sesuai):**
- `input_changed_updates_vil_expr_lint_state`
- `tick_emits_redraw_request_after_debounce`
- `alt_h_on_identifier_opens_popup` (stub message boleh)

**Definition of Done:**
- [ ] Ketika user ketik `vil-expr: unknown_ident`, dalam ≤300ms render menampilkan 1 error line berwarna merah.
- [ ] Ketika user hapus prefix `vil-expr:`, lint state clear, tidak ada overlay tersisa.
- [ ] `cargo nextest run -p vac_tui_runtime --lib` tidak regresi dari baseline saat ini.
- [ ] Tidak ada raw `Color::` di file yang disentuh.

**Commit:** `feat(tui): PR-T12.1 wire vil-expr validator into input + composer render`

**Gotcha:**
- Jangan panggil `lint_expression()` langsung setiap keystroke — itulah alasan debouncenya ada. Selalu lewat `on_input_changed()` + `tick()`.
- Pastikan tick tidak jalan di Tokio runtime thread yang sibuk — taruh di path yang sudah ada, bukan spawn task baru.

---

## 4. PR-W25-1 — Theme migration sweep

**Scope:** ganti ~187 raw `Color::` jadi `state.theme.style(StyleKey::*)` di semua file `services/` + `workbench/`.

**Strategi:** satu commit per file, urut dari kecil ke besar supaya PR reviewable.
1. Inventarisasi: `grep -rn 'Color::' crates/vac_tui_runtime/src/services crates/vac_tui_runtime/src/workbench | grep -v '//' > /tmp/color_migration.txt`
2. Per file: map setiap `Color::<X>` ke `StyleKey`. Kalau butuh key baru, tambah di `services/theme.rs` enum + 3 palette + daftar test `all_presets_cover_all_keys`.
3. Bulk rename via `sed` berbahaya — gunakan ide ini hanya setelah mapping manual dituliskan.

**Commit:** `refactor(tui): migrate <file> to theme StyleKey` per file.

**Exit gate:**
```
grep -rn 'Color::' crates/vac_tui_runtime/src/services crates/vac_tui_runtime/src/workbench | grep -v '//' | wc -l
# → 0
```

**Tests:** `all_presets_cover_all_keys` tetap hijau + semua existing tests tetap hijau.

**Gotcha:**
- `Color::Reset` kadang dibutuhkan untuk clear — jangan map ke `StyleKey` random; buat `StyleKey::Neutral` atau pakai `Style::default()`.
- `Color::Rgb(…)` yang memang brand-color → buat `StyleKey::BrandAccent` dll., jangan keep raw.

---

## 5. PR-W25-7 — Split `event_loop_tests.rs` (1839 → <600)

**Strategi:** per-feature module, harness bersama di `event_loop_tests.rs` (~260 baris).

**Submodul:**
- `event_loop_tests/session.rs` (~420)
- `event_loop_tests/shell.rs` (~320)
- `event_loop_tests/approval.rs` (~280)
- `event_loop_tests/runtime.rs` (~260)
- `event_loop_tests/streaming.rs` (~240)

**Gate:** jumlah `running N tests` sebelum/sesudah IDENTIK. Kalau menurun → stop, pastikan semua `#[test]` ikut dipindah.

**Commit:** `refactor(tui): split event_loop_tests — per-feature modules`

---

## 6. PR-W25-9 — Split `runner.rs` (1347 → <600) + finish async

**Submodul:**
- `runner.rs` (~420) — `Runner::run` main loop saja.
- `runner/session_tasks.rs` (~320) — `switch_to_session`, `resume`, snapshot. **Di sini**: migrasi sisa `block_in_place` ke `_async` variant (`list_snapshots_async`, `has_checkpoint_async`, `cleanup_session_async`).
- `runner/runtime_tasks.rs` (~280) — snapshot polling.
- `runner/backend.rs` (~340) — backend send/recv.

**Bonus:**
- Hapus 3 deprecated warning sisa.
- Exit metric: `grep -rn 'block_in_place' crates/vac_tui_runtime/src/ | wc -l` = 0.

**Tests:** `runner::tests::*` + `scripts/check_sync_io.sh` lulus.

**Commit:** `refactor(tui): split runner.rs + finish session_control async migration`

---

## 7. PR-T11 — VWFD inspector panel

**Deps:** §8 PR-1 (landed), §8 PR-3 (landed), PR-T9 tray.

**Files:**
- `services/vwfd_inspector.rs` (~450) — tree builder + render.
- `handlers/vwfd_input.rs` (~220) — key dispatch.
- Tambahan di `app/types/workbench.rs` (atau file VIL-state yang sesuai): `WorkbenchTab::Vwfd`, `VwfdInspectorState { tree, selected_path, detail_scroll }`.

**Actions baru (register di `action_registry.rs`):**
- `OPEN_VWFD_TAB`
- `VWFD_NODE_UP`, `VWFD_NODE_DOWN`, `VWFD_NODE_EXPAND`
- `VWFD_JUMP_TO_SOURCE` — Enter → emit `OutputEvent::OpenEditor { path, line }` (stub — actual editor integration nanti).

**Layout:** 40% kiri = tree (workflow → step → handler), 60% kanan = detail (source path, trigger type, execution mode badge).

**Tests:**
- `vwfd_tree_builds_from_fixture` (fixture di `crates/vac_tui_runtime/tests/fixtures/sample.vwfd.yaml`)
- `vwfd_select_navigates`
- `vwfd_jump_emits_open_editor`

**Commit:** `feat(tui): PR-T11 VWFD inspector workbench tab`

**Gotcha:** badge execution-mode pakai `StyleKey::*` (tambah key baru kalau perlu: `ExecModeNative`, `ExecModeWasm`, `ExecModeSidecar`).

---

## 8. §8 PR-5b — `vwfd_parity_pass`

**File:** `crates/vil_vwfd/src/parity.rs` (~280).

**API:**
```rust
pub fn parity_pass(vwfd: &Vwfd, rust_root: &Path) -> Vec<ParityIssue>;

pub struct ParityIssue {
    pub kind: ParityIssueKind, // MissingRust | OrphanRust | SignatureMismatch
    pub handler_name: String,
    pub vwfd_path: Option<PathBuf>,
    pub rust_path: Option<PathBuf>,
    pub message: String,
}
```

**Strategi scan:** parse `.vwfd.yaml` handlers, scan Rust `rust_root` untuk `#[vil::handler(name="…")]` via syn.

**Fixtures (wajib 3, di `crates/vil_vwfd/tests/fixtures/parity/`):**
- `full-parity/` — 3 handler match
- `missing-rust/` — VWFD ada, Rust tidak → `MissingRust`
- `orphan-rust/` — Rust ada, VWFD tidak → `OrphanRust`

**Tests:** 1 per fixture + edge case `parity_pass_ignores_non_handler_fns`.

**Commit:** `feat(vil): vwfd_parity_pass (PR-5b)`

---

## 9. §8 PR-6 — `vac_changeset::formats::vwfd` semantic diff

**File:** `crates/vac_changeset/src/formats/vwfd.rs` (~420).

**API:**
```rust
pub fn diff(old: &Vwfd, new: &Vwfd) -> VwfdDiff;

pub struct VwfdDiff {
    pub added_steps: Vec<StepRef>,
    pub removed_steps: Vec<StepRef>,
    pub modified_steps: Vec<StepModification>,
    pub modified_expressions: Vec<ExpressionDelta>, // AST delta via vil_expr
}
```

**Strategi AST delta:** parse expression kedua sisi via `vil_expr::parse`, normalize, recursive compare → emit node-level diff (not text).

**Fixtures (5):**
- `added_step/` — tambah 1 step
- `removed_step/` — hapus 1 step
- `modified_expression/` — `a + b` → `a - b`
- `renamed_handler/` — handler name berubah
- `unchanged/` — no-op, harus return empty `VwfdDiff`

**Commit:** `feat(changeset): VWFD semantic diff (PR-6)`

---

## 10. PR-T13 — VIL-aware diff overlay

**Deps:** §8 PR-5b + §8 PR-6.

**File:** `services/vwfd_diff_render.rs` (~380).

**Integrasi:** di `services/review.rs`, kalau path match `*.vwfd.yaml` → delegate ke `vwfd_diff_render::render(&state, &old, &new)`.

**Layout:** 2-column, kiri AST lama, kanan AST baru, sorot `modified_expressions` dengan `StyleKey::DiffExpressionChanged`.

**Mode toggle `M`:** cycle Native ↔ WASM ↔ Sidecar preview (hanya label, tidak eksekusi).

**Tests:**
- `vwfd_diff_renders_added_step`
- `vwfd_diff_renders_expression_delta`
- `mode_toggle_cycles_three_states`

**Commit:** `feat(tui): PR-T13 VIL-aware semantic diff overlay`

---

## 11. PR-T14 — Background vil dev runner

**Deps:** §8 PR-3 (landed), PR-T9 tray.

**File:** `services/vil_dev_runner.rs` (~320).

**Behavior:**
- Spawn `config.vil.dev_command` via `tokio::process::Command`, capture stdout+stderr line-by-line.
- Stream ke tray item `"vil dev"` + panel Activity.
- Parse `[vil-checkpoint] <session_id> <iso-ts>` → emit `InputEvent::VilCheckpoint { session_id, ts }` → append to `vil_dev_checkpoints` Vec + push activity entry (timestamp + session_id). **Reframe (Task-7 Jalur B):** "session timeline markers" = activity log entries with timestamp; dedicated `SessionTimelineMarker` struct not warranted by spec (single bullet, no UI mockup). Covered by Task-6 activity routing.
- Kill: Esc di tray item → SIGTERM, timeout 5s → SIGKILL.

**Tests:**
- `vil_dev_spawns_and_streams` (spawn `sh -c 'echo hello; sleep 0.1; echo [vil-checkpoint] s1 2026-04-21T00:00:00Z'`)
- `vil_dev_kill_sigterm_then_sigkill` (mock slow child)
- `checkpoint_parser_extracts_session`

**Commit:** `feat(tui): PR-T14 background vil dev runner + checkpoint markers`

**Gotcha:** `tokio::process::Child::kill()` sendiri adalah SIGKILL — pakai `nix::sys::signal::kill(pid, SIGTERM)` dulu, baru fallback SIGKILL.

---

## 12. Wave 4 (paralel-friendly)

### PR-T15 — Inline LSP-style diagnostics
- File: `services/diagnostics_overlay.rs` (~280).
- Reuse `vil_validate` + `LspDiagnostics` event existing.
- Squiggly underline pakai `StyleKey::ValidationError` di markdown code block.
- Test: `diagnostics_underline_renders_squiggly`, `diagnostics_clear_on_file_change`.
- Commit: `feat(tui): PR-T15 inline diagnostics overlay`.

### PR-T16 — Mouse dispatch closure
- File: `handlers/mouse.rs`.
- Complete dispatcher untuk banner, workbench tabs, tray item, review file row (regions sudah dicatat).
- Test: `mouse_click_on_banner_dismisses`, `mouse_click_on_tab_switches`, `mouse_click_on_tray_focuses`.
- Commit: `feat(tui): PR-T16 complete mouse click dispatch`.

### PR-T17 — Kitty image protocol
- File: `services/kitty_image.rs` (~220).
- Detect via env `TERM` + query `\e_Gi=31,s=1,v=1,a=q\e\\` timeout 200ms.
- Fallback: ASCII art atau pesan "requires kitty/wezterm".
- Test: `kitty_detect_times_out_gracefully`, `fallback_renders_ascii`.
- Commit: `feat(tui): PR-T17 kitty image protocol preview`.

### PR-T18 — Command recorder / replay
- File: `services/recorder.rs` (~260).
- Record semua `InputEvent` ke `.vac/recordings/<timestamp>.jsonl`.
- Replay via `vac tui --replay <file>`.
- Rotate di 10 MB, keep 20.
- Test: `round_trip_recording_produces_identical_state`, `rotation_keeps_20_files`.
- Commit: `feat(tui): PR-T18 command recorder + replay`.

### PR-T19 — Custom keybindings
- File: `services/keybindings_loader.rs` (~180).
- Source: `.vac/keybindings.toml`, map `action_id` → `KeyEvent`.
- Fallback ke default `action_registry`.
- Test: `custom_binding_overrides_default`, `invalid_binding_emits_warning_not_crash`.
- Commit: `feat(tui): PR-T19 custom keybindings loader`.

---

## 13. Audit rubric (yang akan saya pakai sebagai reviewer)

Untuk setiap PR yang local agent klaim "done", saya akan audit dengan 4 lensa berikut:

### (a) Code reviewer lens
- [ ] Commit message **exact** match spec (tidak boleh paraphrase).
- [ ] Diff stat: files tersentuh hanya yang ada di scope PR (tidak ada drive-by changes).
- [ ] Tidak ada `unwrap()` di production path (test code boleh).
- [ ] Tidak ada `Color::` baru di file apa pun.
- [ ] Tidak ada `#[allow(dead_code)]` / `#[allow(unused)]` baru tanpa justifikasi di komentar.
- [ ] Tidak ada `println!` / `eprintln!` di production path (pakai `tracing`).

### (b) Software architect lens
- [ ] File baru tidak melanggar layering (`services/` tidak import `handlers/`, dst).
- [ ] Surface public modul sesuai spec — tidak menambah ekspor yang tidak dibutuhkan.
- [ ] Dependency baru di `Cargo.toml` direview: versi pinned, feature-gated kalau opsional.
- [ ] Tidak memperkenalkan sync I/O di path async (verifikasi via `scripts/check_sync_io.sh`).
- [ ] File hasil split <600 baris (target arsitektur Wave 2.5).

### (c) Software engineer lens (eksekusi + test)
- [ ] **Semua nama test** di spec PR hadir dan hijau (saya akan grep).
- [ ] `cargo nextest run -p <crate> --lib` tidak menurunkan jumlah test (except W25-7 split yang harus IDENTIK).
- [ ] Edge case: empty input, malformed input, concurrent input (kalau relevan) ditest.
- [ ] Dashboard di-update di commit yang sama (kolom metric sesuai scope).

### (d) Operasional lens
- [ ] Tidak regresi di 2 pre-existing failing test (privacy AWS ARN, scheduler monitor_mode) — jumlah failure tetap 2, bukan 3+.
- [ ] Tidak ada stale `REBASE_HEAD` / merge artifact setelah commit.
- [ ] Tidak ada file untracked yang seharusnya di `.gitignore`.

### Format laporan audit

Setelah local agent submit PR, saya akan balas dengan format:

```
## Audit PR-<ID>

### (a) Code review
- ✅ / ❌ Commit message
- ✅ / ❌ Scope bersih
- …

### (b) Arsitektur
- …

### (c) Engineering
- …

### (d) Operasional
- …

### Verdict: ACCEPT / REJECT / ACCEPT WITH FIXUP
### Required fixups (kalau ada)
1. …
```

---

## 14. Catatan eksekusi

- **Jangan** skip fase "audit gap" di §1 (LOC W25-4/5/6 + `watch_theme` test + `grep Color::` baseline). Hasilnya menentukan apakah W25-1/4/5/6 bisa ditandai done tanpa rework.
- **Urutan commit masuk origin**: tidak harus push; cukup commit lokal. Saya audit dari diff HEAD~N..HEAD.
- **Kalau ragu split mana yang dipakai** (spec vs implementasi eksisting): gunakan yang sudah ada jika LOC <600 per submodul — jangan re-split. Dashboard catat deviasi struktur di kolom "Notes".
- **PR-T12.1 sengaja PR kecil** — kerjakan paralel dengan W25-1 supaya tidak memblok Wave 3.
- Kalau local agent menemukan gap spec (mis. API VWFD berubah setelah §8 PR-1), **hentikan PR**, dokumentasikan di dashboard, dan eskalasi — jangan lanjut dengan asumsi.
