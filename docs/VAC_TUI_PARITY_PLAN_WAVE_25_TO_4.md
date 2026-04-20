# VAC TUI Parity — Execution Plan Wave 2.5 → 3 → 4

> Addendum untuk `docs/VAC_TUI_PARITY_PLAN.md`. Dibuat 2026-04-20 setelah commit `302c99c` (update.rs split).
>
> Konteks entry: 12 file >600 baris tersisa, tema parsial (404 raw `Color::` di 21 file, belum ada TOML/notify), §8 PR-2..6 belum ditutup, 191 tests green baseline.

## Prinsip eksekusi

1. Satu PR = satu commit kecuali split bertahap (extract → adopt → remove legacy).
2. Tidak ada raw `Color::` di file baru — wajib `state.theme.style(StyleKey::*)`.
3. `scripts/check_sync_io.sh` lulus per commit.
4. `cargo test -p vac_tui_runtime --lib` tidak regresi dari 191 green.
5. Setiap PR yang menyentuh file >600 baris menurunkannya atau tidak menambah.
6. PR yang menyentuh surface VIL refer ke `docs/VAC_SECTION_8_IMPLEMENTATION_PLAN.md`.

---

## Wave 2.5 — Gate ke Wave 3 (3–5 hari)

**Exit**: >=6 file turun <600, services+workbench nol raw Color, PR-T5 punya TOML+notify, tests tetap >=191 green.

### PR-W25-1 — Theme migration sweep

- Scope: 404 raw `Color::` di 21 file → `StyleKey::*`.
- Tambah `StyleKey` yang belum ada di `services/theme.rs` (mis. `RuntimeBadgeError`, `ShellPromptReady`).
- Urutan (kecil ke besar):
  1. `services/ask_user.rs` (676)
  2. `services/review.rs` (675)
  3. `services/text_selection.rs` (714)
  4. `services/vil_workbench.rs` (770)
  5. `services/clipboard_paste.rs` (871)
  6. `services/shortcuts_popup.rs` (996)
  7. `services/markdown_renderer.rs` (2010) — dikerjakan bersamaan dengan PR-W25-5.
- Commit per file: `refactor(tui): migrate <file> to theme StyleKey`.
- Verifikasi: `grep -rn 'Color::' crates/vac_tui_runtime/src/services crates/vac_tui_runtime/src/workbench | grep -v '//' | wc -l` = 0.

### PR-W25-2 — PR-T5 closeout (TOML + hot-reload)

- File baru: `services/theme_loader.rs` (~220 baris).
  - `load_theme_toml(path) -> Result<ThemeConfig>`
  - `watch_theme(path, tx) -> JoinHandle` pakai `notify` v6, debounce 250ms, emit `InputEvent::ThemeReloaded`.
  - Lookup: `$VAC_THEME` → `.vac/theme.toml` → `~/.config/vac/theme.toml` → built-in.
- Integrasi: arm `InputEvent::ThemeReloaded(theme)` di `update.rs`.
- Deps: tambah `notify = "6"` + `toml = "0.8"` di workspace `Cargo.toml`.
- Test: parses_sample_toml, falls_back_to_builtin, watch_theme_emits_within_500ms (tempdir).
- Commit: `feat(tui): PR-T5 TOML theme loader + notify hot-reload`.

### PR-W25-3 — Split `services/shortcuts_popup.rs` (996 → <600)

- `services/shortcuts_popup.rs` (~380) state + render + event.
- `services/shortcuts_popup/catalog.rs` (~350) static shortcut entries.
- `services/shortcuts_popup/search.rs` (~260) fuzzy filter (reuse `SkimMatcherV2`).
- Re-export `pub use catalog::*; pub use search::*;`.
- Test tambahan: `catalog_contains_expected_groups`, `search_filters_by_fuzzy_score`.
- Commit: `refactor(tui): split shortcuts_popup — catalog + search submodules`.

### PR-W25-4 — Split `handlers/input_popup.rs` (1176 → <600)

- `handlers/input_popup.rs` (~320) dispatcher + `refresh_session_resume_filtered`.
- `handlers/input_popup/session_resume.rs` (~280) PR-T8 fuzzy + filter + Ctrl+R.
- `handlers/input_popup/file_search.rs` (~220) PR-T6.
- `handlers/input_popup/at_mention.rs` (~190) PR-T7.
- `handlers/input_popup/model_switcher.rs` (~180).
- Test existing `session_resume_*`, `file_search_*`, `at_mention_*` harus hijau tanpa pindah lokasi.
- Commit: `refactor(tui): split input_popup — one module per overlay kind`.

### PR-W25-5 — Split `services/markdown_renderer.rs` (2010 → <600)

- `services/markdown_renderer.rs` (~420) public API + dispatcher.
- `services/markdown_renderer/code_block.rs` (~380) syntect.
- `services/markdown_renderer/tables.rs` (~260).
- `services/markdown_renderer/lists.rs` (~220).
- `services/markdown_renderer/inline.rs` (~340).
- `services/markdown_renderer/diff.rs` (~280).
- Dep: setelah PR-W25-1 sudah sentuh markdown_renderer theme.
- Test snapshot existing tetap hijau + tambah `code_block_highlights_rust`, `table_renders_with_header_separator`.
- Commit: `refactor(tui): split markdown_renderer — per-block submodules`.

### PR-W25-6 — Split `view.rs` (1921 → <600)

- `view.rs` (~420) layout orchestration.
- `view/header.rs` (~180) status bar + token + VIL score.
- `view/messages.rs` (~380) transcript + streaming.
- `view/workbench.rs` (~420) 4 tab renders.
- `view/input.rs` (~240) input + hints + @-chip highlight.
- `view/overlays.rs` (~320) popup renders.
- `view/footer.rs` (~120) keybind hint.
- Dep: setelah PR-W25-1 + PR-W25-5.
- Test: `view::tests::*` (5 tests) hijau, pindah ke `view/tests.rs` bila perlu.
- Commit: `refactor(tui): split view.rs — per-area submodules`.

### PR-W25-7 — Split `event_loop_tests.rs` (1839 → <600)

- `event_loop_tests.rs` (~260) common harness + mod decl.
- `event_loop_tests/session.rs` (~420).
- `event_loop_tests/shell.rs` (~320).
- `event_loop_tests/approval.rs` (~280).
- `event_loop_tests/runtime.rs` (~260).
- `event_loop_tests/streaming.rs` (~240).
- Gate: count `running N tests` before/after identik.
- Commit: `refactor(tui): split event_loop_tests — per-feature modules`.

### PR-W25-8 — Split `app/types.rs` (1570 → <600)

- `app/types.rs` (~380) AppState + re-exports.
- `app/types/runtime.rs` (~280).
- `app/types/workbench.rs` (~220).
- `app/types/shell.rs` (~240).
- `app/types/review.rs` (~220).
- `app/types/vil.rs` (~220).
- Dep: setelah 6 split lain supaya surface stabil.
- Commit: `refactor(tui): split app/types.rs — per-feature submodules`.

### PR-W25-9 — Split `runner.rs` (1347 → <600) + async migration

- `runner.rs` (~420) Runner::run main loop.
- `runner/session_tasks.rs` (~320) switch_to_session, resume, snapshot — sekaligus migrasi `vac_session_control::list_snapshots`/`has_checkpoint`/`cleanup_session` ke variant `_async`.
- `runner/runtime_tasks.rs` (~280) snapshot polling.
- `runner/backend.rs` (~340) backend send/recv.
- Bonus: hilangkan 3 deprecated warning.
- Commit: `refactor(tui): split runner.rs + migrate session_control to async`.

### Wave 2.5 landing order

```
W25-1 (theme sweep per-file)
  ↓
W25-2 (PR-T5 TOML + notify)
  ↓
W25-3 shortcuts_popup    W25-4 input_popup    W25-5 markdown_renderer
                                                    ↓
                                              W25-6 view.rs
                                                    ↓
W25-7 event_loop_tests   W25-9 runner.rs
                                ↓
                         W25-8 app/types.rs (terakhir, surface stabil)
```

Exit gate Wave 2.5: `wc -l ... | awk '$1>600'` <= 6 baris (target 0).

---

## Wave 3 — VIL-Native Surfaces (7–10 hari)

Gate masuk: Wave 2.5 selesai + §8 PR-2 + §8 PR-3 landed.

### §8 PR-2 — `crates/vil_expr` (prereq PR-T12)

- Files: `Cargo.toml`, `src/lib.rs`, `src/parser.rs`, `src/validator.rs`.
- Strategi minimal (grammar belum final): parser placeholder — identifier, dot-access, call, literal, binary op.
- `fn validate(source: &str, symbols: &SymbolTable) -> ValidationReport` dengan issue `{line, col, severity, message}`.
- 8 fixture (valid + invalid).
- Mark `#[unstable]` di lib-level doc agar jelas placeholder.
- Commit: `feat(vil): vil_expr crate — parser + validator skeleton (PR-2)`.

### §8 PR-3 — `VacConfig::vil` section (prereq PR-T11/T14)

- File: `crates/vac_core/src/config.rs`.
- Field: `vil: VilConfig { vwfd_paths: Vec<PathBuf>, dev_command: Option<String>, checkpoint_interval_secs: u64 }`.
- Default: `["./workflows/**/*.vwfd.yaml"]`, `Some("vac vil dev")`, 300.
- Test: `vac_config_parses_vil_section`, `vil_section_defaults`.
- Commit: `feat(core): VacConfig::vil section for VWFD + dev command`.

### PR-T11 — VWFD inspector panel

- Deps: §8 PR-1 (landed), §8 PR-3, PR-T9 (tray landed).
- Files baru: `services/vwfd_inspector.rs` (~450), `handlers/vwfd_input.rs` (~220).
- State: `WorkbenchTab::Vwfd`, `VwfdInspectorState { tree, selected_path, detail_scroll }`.
- Render: 40% tree kiri (workflows → steps → handlers), 60% detail kanan (source path, trigger, execution mode badge).
- Actions: `OPEN_VWFD_TAB`, `VWFD_NODE_UP/DOWN/EXPAND`, `VWFD_JUMP_TO_SOURCE`.
- Jump: Enter → `OutputEvent::OpenEditor(path, line)` (stub editor integration).
- Test: `vwfd_tree_builds_from_fixture`, `vwfd_select_navigates`, `vwfd_jump_emits_open_editor`.
- Commit: `feat(tui): PR-T11 VWFD inspector workbench tab`.

### PR-T12 — vil-expr live validator in input

- Deps: §8 PR-2.
- File: `services/vil_expr_lint.rs` (~260).
- Hook: `handlers/input.rs` on `InputChanged` → kalau pattern `vil-expr:` match, debounce 200ms, panggil `vil_expr::validate`.
- Render: `AppState::vil_expr_issues`, overlay warna via `StyleKey::ValidationError/Ok`.
- Hover Alt+H di atas identifier → popup type inferred.
- Test: `lint_detects_unknown_identifier`, `lint_ok_on_valid_expression`, `lint_debounces_rapid_typing`.
- Commit: `feat(tui): PR-T12 vil-expr live validator in input`.

### §8 PR-5b — `vwfd_parity_pass` (prereq PR-T13)

- File: `crates/vil_vwfd/src/parity.rs` (~280).
- `fn parity_pass(vwfd, rust_root) -> Vec<ParityIssue>` cek handler VWFD <-> macro `#[vil::handler]` di Rust.
- 3 fixture: full-parity, missing-rust, orphan-rust.
- Commit: `feat(vil): vwfd_parity_pass (PR-5b)`.

### §8 PR-6 — `vac_changeset::formats::vwfd` semantic diff (prereq PR-T13)

- File: `crates/vac_changeset/src/formats/vwfd.rs` (~420).
- `fn diff(old, new) -> VwfdDiff { added_steps, removed_steps, modified_steps, modified_expressions (AST delta) }`.
- 5 fixture diff scenarios.
- Commit: `feat(changeset): VWFD semantic diff (PR-6)`.

### PR-T13 — VIL-aware diff overlay

- Deps: §8 PR-5b + PR-6.
- File: `services/vwfd_diff_render.rs` (~380).
- Integrasi: `services/review.rs` detect `*.vwfd.yaml` → delegate.
- Render: kolom kiri AST lama, kanan AST baru, sorot expression delta.
- Mode toggle `M` → preview Native ↔ WASM ↔ Sidecar.
- Test: `vwfd_diff_renders_added_step`, `vwfd_diff_renders_expression_delta`, `mode_toggle_cycles_three_states`.
- Commit: `feat(tui): PR-T13 VIL-aware semantic diff overlay`.

### PR-T14 — Background vil dev integration

- Deps: §8 PR-3, PR-T9.
- File: `services/vil_dev_runner.rs` (~320).
- Spawn `dev_command` via `tokio::process`, stream ke tray + Activity.
- Checkpoint parser: log line `[vil-checkpoint] <session> <ts>` → `InputEvent::VilCheckpoint` → dot di PR-T8 session timeline.
- Kill: Esc di tray item → SIGTERM, 5s → SIGKILL.
- Test: `vil_dev_spawns_and_streams`, `vil_dev_kill_sigterm_then_sigkill`, `checkpoint_parser_extracts_session`.
- Commit: `feat(tui): PR-T14 background vil dev runner + checkpoint markers`.

### Wave 3 landing order

```
§8 PR-2              §8 PR-3
   ↓                   ↓
 PR-T12              PR-T11
                        ↓
§8 PR-5b        §8 PR-6
   ↓                   ↓
         PR-T13
            ↓
         PR-T14 (juga butuh PR-T9 + §8 PR-3)
```

Exit kriteria Wave 3: PR-T11..T14 landed, VWFD workflow fully inspectable + lintable + diffable + runnable dari TUI tanpa keluar.

---

## Wave 4 — Polish (3–5 hari, stretch)

### PR-T15 — Inline LSP-style diagnostics

- File: `services/diagnostics_overlay.rs` (~280).
- Reuse `vil_validate` + `LspDiagnostics` event existing.
- Squiggly underline di markdown code block saat file path match.
- Commit: `feat(tui): PR-T15 inline diagnostics overlay`.

### PR-T16 — Mouse dispatch closure (Phase 6.5)

- File: `handlers/mouse.rs`.
- Click regions sudah tercatat; lengkapi dispatcher untuk banner, workbench tabs, tray item, review file row.
- Test: `mouse_click_on_banner_dismisses`, `mouse_click_on_tab_switches`, `mouse_click_on_tray_focuses`.
- Commit: `feat(tui): PR-T16 complete mouse click dispatch`.

### PR-T17 — Kitty image protocol

- File: `services/kitty_image.rs` (~220).
- Detect via env `TERM` + query `\e_Gi=31,s=1,v=1,a=q\e\\` timeout 200ms.
- Fallback: ASCII art atau pesan "requires kitty/wezterm".
- Commit: `feat(tui): PR-T17 kitty image protocol preview`.

### PR-T18 — Command recorder / replay

- File: `services/recorder.rs` (~260).
- Record semua `InputEvent` ke `.vac/recordings/<timestamp>.jsonl`.
- Replay via `vac tui --replay <file>`.
- Rotate di 10 MB, keep last 20 recordings.
- Test: round-trip recording produces identical state.
- Commit: `feat(tui): PR-T18 command recorder + replay`.

### PR-T19 — Custom keybindings

- File: `services/keybindings_loader.rs` (~180).
- Source: `.vac/keybindings.toml`, map `action_id` → `KeyEvent`.
- Fallback ke default `action_registry`.
- Test: `custom_binding_overrides_default`, `invalid_binding_emits_warning_not_crash`.
- Commit: `feat(tui): PR-T19 custom keybindings loader`.

### Wave 4 landing order

Paralel-friendly. Rekomendasi: T16 → T15 → T19 → T17 → T18.

---

## Cross-wave risks & mitigations

| Risk | Wave | Mitigation |
|---|---|---|
| Split invasif (view.rs, app/types.rs) memecah cross-ref | 2.5 | Lakukan setelah semua split kecil; `pub use` re-export supaya pemanggil tidak ikut berubah |
| `vil_expr` grammar belum final | 3 | PR-2 pakai placeholder dengan `#[unstable]`, siap di-swap |
| `notify` debounce quirks (macOS FSEvent) | 2.5 | Debounce 250ms di kode kita sendiri, pakai `RecommendedWatcher::new_immediate` |
| Kitty fingerprint berbeda per terminal | 4 | Feature-detect query timeout 200ms, fallback aman |
| Recorder file membengkak | 4 | Rotate 10MB, keep 20 |

## Metrik kemajuan

Tiap commit update dashboard `docs/VAC_TUI_PARITY_DASHBOARD.md` (buat baru jika belum ada):

```
<commit-sha> | <date> | <files >600> | <tests> | <raw Color::> | <wave PR>
```

## Exit keseluruhan (akhir Wave 4)

- 0 file >600 baris di `crates/vac_tui_runtime/src/`.
- 0 raw `Color::` di `crates/vac_tui_runtime/src/`.
- PR-T11..T14 functional.
- Wave 4 PR landed semua, atau ADR per PR yang sengaja di-skip.
- `cargo test --workspace` hijau.
