# Superbatch E — Finalize Wave 5 Spec

## Why
Setelah Superbatch D, Wave 5 sudah jauh lebih mature tetapi masih ada sisa nyata: dual-path review surface (legacy popup vs workstation), autopilot yang masih wrapper lifecycle (bukan controller), dan belum ada bukti integrasi end-to-end untuk perbedaan mode monitor vs auto serta state `waiting_approval`.

## What Changes
- Unifikasi Review Workstation: hapus jalur legacy `show_file_changes_popup` dan seluruh state/handler terkait, sehingga hanya ada satu surface review resmi.
- Autopilot Controller Phase 2: jadikan `vac autopilot up` menjalankan controller nyata (bukan spawn `runtime start`), dengan loop lifecycle, state heartbeat, dan behavior mode monitor/auto yang benar-benar berbeda.
- Proof integrasi E2E: tambah integration tests yang membuktikan review workstation dan autopilot berperilaku operasional sesuai mode, termasuk transisi `waiting_approval`.
- Surface cleanup & gate tightening: rapikan hint/shortcut agar sinkron, harden JSON status autopilot, dan pastikan slash semantics tidak regress.

## Impact
- Affected specs: Review workstation, Runtime/queue, Autopilot orchestration, CLI automation surface (json), Testing & gates.
- Affected code:
  - `crates/vac_cli/src/tui/event_loop.rs`
  - `crates/vac_cli/src/tui/event.rs`
  - `crates/vac_cli/src/tui/view.rs`
  - `crates/vac_cli/src/tui/app/types.rs`
  - `crates/vac_cli/src/tui/app/events.rs`
  - `crates/vac_cli/src/commands/autopilot.rs`
  - `crates/vac_runtime/src/scheduler.rs`
  - `crates/vac_runtime/src/queue.rs`
  - `crates/vac_runtime/src/lib.rs`
  - **File baru** (direkomendasikan): `crates/vac_runtime/src/autopilot.rs`
  - Tests: `crates/vac_cli/tests/*`, `crates/vac_runtime/tests/*`

## ADDED Requirements

### Requirement: Review Workstation Unification (E1)
Sistem SHALL memiliki satu-satunya surface review resmi yang digunakan untuk seluruh aksi review.

#### Scenario: Single Review Surface
- **WHEN** user menjalankan `/review` atau shortcut review (mis. Ctrl+G)
- **THEN** sistem membuka review workstation (`review_open`) dan tidak pernah membuka legacy popup.

#### Scenario: No Legacy Operational Path
- **WHEN** sistem berjalan normal (TUI loop)
- **THEN** tidak ada logic operasional yang bergantung pada `show_file_changes_popup`, `file_changes_selected`, `file_changes_search`, atau handler legacy terkait.

#### Scenario: Review Actions Only in Workstation
- **WHEN** user melakukan: open/close/filter/selection/diff toggle/revert selected/revert filtered/revert all/open editor
- **THEN** semua aksi tersebut hanya dieksekusi lewat state dan event review workstation (`review_*`) tanpa jalur duplikat.

### Requirement: Autopilot Controller Phase 2 (E2)
Sistem SHALL menjalankan autopilot sebagai controller nyata, bukan wrapper yang spawn `vac runtime start`.

#### Scenario: Controller Lifecycle Loop
- **WHEN** `vac autopilot up` dijalankan
- **THEN** controller:
  - load config (`autopilot.toml`)
  - open persistent queue (`.vac/queue.json`)
  - publish state heartbeat ke `.vac/autopilot.state`
  - mendelegasikan eksekusi job sesuai mode
  - menangani shutdown (`vac autopilot down`) dengan rapi

#### Scenario: Mode Monitor vs Auto (Observable)
- **WHEN** mode = `monitor`
- **THEN** job tidak didequeue/dieksekusi; state tetap bergerak (polling/idle) dan `last_event` minimal menunjukkan `task_queued` jika queue berisi.
- **WHEN** mode = `auto`
- **THEN** controller men-dequeue dan mengeksekusi job; state dan `last_event` berubah sesuai transisi operasional.

#### Scenario: Waiting Approval Is Real
- **WHEN** job memerlukan approval tool
- **THEN** `.vac/autopilot.state` berpindah ke `waiting_approval` dan `last_event` menjadi `approval_needed` hingga approval diproses.

#### Scenario: State File Ownership
- **WHEN** autopilot berjalan via controller
- **THEN** `.vac/autopilot.state` diperbarui oleh controller (bukan sekadar efek samping scheduler loop generik).

### Requirement: End-to-End Integration Proof (E3)
Sistem SHALL memiliki integration tests yang membuktikan behavior operasional, bukan hanya unit test.

#### Scenario: Review Workstation E2E
- **WHEN** `/review` dijalankan di TUI flow (test harness)
- **THEN** workstation terbuka dan aksi revert selected/filtered/all memodifikasi working tree sesuai snapshot, serta open editor tidak panic ketika editor ada/tidak ada.

#### Scenario: Autopilot Monitor E2E
- **WHEN** queue berisi job dan controller berjalan di mode monitor
- **THEN** job tetap queued; state file menunjukkan `polling` dan `last_event=task_queued`.

#### Scenario: Autopilot Auto E2E
- **WHEN** queue berisi job dan controller berjalan di mode auto
- **THEN** job dieksekusi sampai selesai/gagal; state file menunjukkan transisi state + `last_event` yang berubah nyata.

#### Scenario: Waiting Approval E2E
- **WHEN** job yang memerlukan approval diproses
- **THEN** state file mencapai `waiting_approval` dan dapat diverifikasi oleh test.

#### Scenario: Lifecycle E2E
- **WHEN** menjalankan `autopilot up` → `autopilot status` → `autopilot down`
- **THEN** PID/state cleanup benar dan status tidak misleading.

### Requirement: Surface Cleanup & Gate Tightening (E4)
Sistem SHALL menjaga konsistensi surface yang aktif dan stabilitas automation output.

#### Scenario: Slash Semantics No Regression
- **WHEN** `/fix` dan `/explain` dipanggil dengan argumen
- **THEN** prompt semantik dikirim (bukan literal slash) dan behavior konsisten antara input bar dan command palette.

#### Scenario: Autopilot Status JSON Contract
- **WHEN** `vac autopilot status --format json`
- **THEN** payload memuat field-field penting untuk audit mesin dan tetap kompatibel dengan perubahan internal.

## MODIFIED Requirements

### Requirement: Review Shortcut & Hints
Shortcut dan hint review SHALL merepresentasikan satu-satunya workstation review yang aktif, tanpa menyebut surface legacy.

### Requirement: Autopilot Daemon Semantics
`vac autopilot up` SHALL menjalankan controller dan bukan spawn runtime sebagai mekanisme utama.

## REMOVED Requirements

### Requirement: Legacy Review Popup Operational Path
**Reason**: dual-path review menciptakan ambiguity, logic ganda, dan risk regress.  
**Migration**: semua aksi review dipindahkan ke workstation review (`review_open`).

