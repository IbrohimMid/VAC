# Tasks

- [x] Task 1: C0 — Create competitive blueprint docs
  - [x] Buat file `docs/competitive/CODEX_TUI_DONOR_MAP.md` dengan detail fitur Codex, equivalent di VAC, dan allowed/forbidden donor patterns.
  - [x] Buat file `docs/competitive/CODEX_PARITY_BLUEPRINT.md` dengan blueprint C-TRACK.
  - [x] Buat file `docs/runtime-integration/C_TRACK_STATUS.md` untuk melacak status (NOT STARTED/IMPLEMENTED/dll) dari slice C1-C12.

- [x] Task 2: C1 — Codex-grade CLI grammar
  - [x] Tambahkan alias `vac chat` untuk flow interactive di `crates/vac_cli`.
  - [x] Tambahkan alias `vac exec <prompt>` (termasuk `--ephemeral`, `--sandbox read-only|workspace-write|danger-full-access`).
  - [x] Tambahkan command `vac sandbox status` dan `vac sandbox doctor`.

- [x] Task 3: C2 — Default TUI `/init` parity
  - [x] Tambahkan alias `/init` ke `vac_tui_runtime`.
  - [x] Render minimal overlay atau workbench panel dengan readiness rows (Model, Sandbox, Sessions, Doctor, MCP, Status, Logs).

- [x] Task 4: C3 — Sandbox UX normalization
  - [x] Definisikan enum `UserSandboxMode` (ReadOnly, WorkspaceWrite, DangerFullAccess).
  - [x] Hubungkan CLI flags, TUI statusline, `/sandbox`, dan `/isolation`.
  - [x] Implementasikan konfirmasi eksplisit untuk mode danger.

- [x] Task 5: C4 — AppEvent-style dispatcher scaffold
  - [x] Buat file `crates/vac_tui_runtime/src/app_event.rs` dengan enum `VacAppEvent`.
  - [x] Rute setidaknya satu path (seperti `ShowInitChecklist` atau `DiffResult`) melalui `VacAppEvent`.

- [x] Task 6: C5 — Transcript cell minimal layer
  - [x] Buat modul history cells di `crates/vac_tui_runtime/src/history_cell/`.
  - [x] Buat adapter dari `Message`/`RuntimeUpdate` ke `VacHistoryCell`.
  - [x] Render minimal untuk conversation lane (tool call cards, error, info).

- [x] Task 7: C6 — Streaming fidelity audit + patch
  - [x] Audit `VacEngineAdapter` dan perbaiki incremental UI updates untuk text delta dan tool calls.

- [x] Task 8: C7 — Plan mode hard gate
  - [x] Hubungkan state `PlanState.mode_active` dengan pemblokiran tool destruktif.
  - [x] Tampilkan status plan mode di UI.

- [x] Task 9: C8 — Todo conversation lane
  - [x] Render Todo checklist di conversation lane TUI menggunakan state yang ada.

- [x] Task 10: C9 — Resume/fork UX
  - [x] Tambahkan atau perbaiki fungsionalitas `/resume`, `/fork`, dan `/sessions`.

- [x] Task 11: C10 — Diff/review overlay polish
  - [x] Pastikan `/review` atau `/diff` membuka diff/review surface yang stabil setelah perubahan file.

- [x] Task 12: C11 — MCP inventory config
  - [x] Tambahkan dukungan konfigurasi MCP server approval defaults di TOML config.

- [x] Task 13: C12 — Notify hook
  - [x] Tambahkan dokumentasi dan implementasi minimal untuk hook notifikasi (`turn_finished`, dll).

# Task Dependencies
- Task 2-13 dapat dijalankan sebagian besar secara independen, namun disarankan dikerjakan sesuai urutan C1-C12.
- Task 1 harus diselesaikan terlebih dahulu untuk menyiapkan dokumentasi baseline.
