# Wave 5 Superbatch (Patch D + Maturity Phase 1) Spec

## Why
Wave 5 corrective gaps sudah PASS sampai Patch C, tetapi maturity follow-ups (review workstation completeness, test gates, dan autopilot phase 1) masih membutuhkan pengerasan agar product surface benar-benar usable dan bisa diaudit di `main` tanpa oversell.

## What Changes
- Menambah test suite yang benar-benar memvalidasi review workstation (state transitions, revert behavior, diff loader) dan slash semantics yang sudah ada.
- Merapikan desain event review agar tidak ada event “pinjaman” dan mapping keyboard konsisten dengan UI hints.
- Membuat diff scroll benar-benar dipakai end-to-end (state → event → render).
- Memoles review workstation agar usable end-to-end: lazy diff refresh, status/error per file terlihat, editor spawn aman dengan fallback.
- Memperkenalkan Autopilot Maturity Phase 1: state machine lebih kaya, `.vac/autopilot.state` lebih actionable, event taxonomy minimal, polling interval nyata, dan mode monitor vs auto observable.
- Hardening tambahan: konsistensi path queue/runtime, konsistensi permission mode messaging/hints, dan test gate yang paling load-bearing untuk perubahan di atas.
- **BREAKING**: remap sebagian shortcut TUI untuk review workstation bila sebelumnya memakai event lain (contoh: tidak lagi “numpang” event non-review).

## Impact
- Affected specs: TUI Review Workstation, Operator UX, Runtime/Queue Persistence, Autopilot Orchestration, Testing & Gates
- Affected code (perkiraan):  
  - `crates/vac_cli/src/tui/app/types.rs`  
  - `crates/vac_cli/src/tui/app/events.rs`  
  - `crates/vac_cli/src/tui/event.rs`  
  - `crates/vac_cli/src/tui/event_loop.rs`  
  - `crates/vac_cli/src/tui/view.rs`  
  - `crates/vac_cli/src/tui/services/review.rs`  
  - `crates/vac_cli/src/commands/autopilot.rs` (atau lokasi command autopilot aktual)  
  - `crates/vac_runtime/src/scheduler.rs` dan modul runtime terkait  
  - `crates/vac_core/src/config.rs` (bila perlu untuk mode/interval)  
  - `crates/vac_cli/tests/*` dan/atau unit test inline yang relevan

## ADDED Requirements

### Requirement: Review Workstation Tests (Patch D)
Sistem SHALL menyediakan test yang memvalidasi review workstation dan behavior inti yang paling risk-prone.

#### Scenario: Selection & Filter Normalization
- **WHEN** filter berubah dan membuat item terpilih tidak lagi ada di projection
- **THEN** selection ternormalisasi (tidak out-of-bounds) dan `review_selected_path` konsisten dengan projection

#### Scenario: Revert State Update
- **WHEN** user melakukan revert selected / filtered / all
- **THEN** `modified_files` dan `review_items` ter-update konsisten, termasuk status/error per file

#### Scenario: Diff Loader Snapshot vs Working Tree
- **WHEN** snapshot ada atau tidak ada untuk file tertentu
- **THEN** `load_diff` mengembalikan `old_content/new_content` sesuai fallback yang terdefinisi (string kosong bila sisi tidak ada)

#### Scenario: Review Open/Close Transitions
- **WHEN** workstation dibuka dan ditutup
- **THEN** state review (`review_open`, diff state, selection) berubah sesuai contract tanpa kebocoran state

#### Scenario: Built-in Slash Semantics
- **WHEN** user menjalankan `/review`, `/fix`, `/explain` dari input bar
- **THEN** perilaku sesuai semantics: `/review` masuk workstation, `/fix` dan `/explain` mengirim prompt-content + args (bukan literal slash)

### Requirement: Clean Review Events (No Borrowed Events)
Sistem SHALL memiliki event review yang eksplisit untuk setiap aksi workstation (termasuk revert filtered), tanpa memanfaatkan event lain yang tidak semantik.

#### Scenario: Revert Filtered
- **WHEN** user memicu revert filtered dari shortcut atau UI
- **THEN** event yang diproses adalah `ReviewRevertFiltered` (bukan event non-review)

### Requirement: Diff Scroll End-to-End
Sistem SHALL menerapkan scroll diff end-to-end.

#### Scenario: Scroll Diff
- **WHEN** user melakukan scroll pada panel diff
- **THEN** `ReviewDiffState.scroll` berubah dan render diff menampilkan viewport yang ter-scroll

### Requirement: Review Workstation Polish
Sistem SHALL menyediakan workstation yang usable end-to-end untuk operator.

#### Scenario: Error Visibility
- **WHEN** restore snapshot gagal untuk sebuah file
- **THEN** error terlihat di UI per-file (bukan hanya assistant message)

#### Scenario: Editor Spawn Safe
- **WHEN** user memilih Open Editor
- **THEN** editor dipilih dari `VAC_EDITOR` lalu `EDITOR`, fallback ke `nvim/vim/nano` bila env kosong, dan TUI suspend/restore aman tanpa crash

### Requirement: Autopilot Maturity Phase 1
Sistem SHALL memperkaya autopilot menjadi orchestration layer yang lebih observable dan actionable, tanpa mengubah scope Wave 6.

#### Scenario: Rich State File
- **WHEN** autopilot berjalan
- **THEN** `.vac/autopilot.state` berisi minimal: `state`, `mode`, `poll_interval_secs`, `queue_len`, `current_job`, `last_event`, `last_error`, `updated_at`

#### Scenario: Rich Status Command
- **WHEN** user menjalankan `vac autopilot status`
- **THEN** status menampilkan field-field di atas secara jelas (format text dan/atau json mengikuti flag yang ada)

#### Scenario: Poll Interval Is Real
- **WHEN** `poll_interval_secs` di-config
- **THEN** loop/tick autopilot benar-benar menggunakan interval tersebut (bukan hanya ditampilkan)

#### Scenario: Mode Observable
- **WHEN** `mode=monitor` vs `mode=auto`
- **THEN** ada perbedaan perilaku yang bisa diaudit (misalnya auto mengeksekusi job, monitor hanya melaporkan/menjaga state tanpa eksekusi penuh, sesuai batasan implementasi phase 1)

## MODIFIED Requirements

### Requirement: Review Workstation Interaction Model
Workstation review SHALL menjadi jalur utama untuk `/review` dan shortcut review, menggantikan popup file-changes lama sebagai surface utama.

### Requirement: Keyboard Hint Consistency
TUI SHALL menampilkan hints yang sinkron dengan mapping event yang benar-benar aktif.

## REMOVED Requirements

### Requirement: Borrowed Review Action Events
**Reason**: event “pinjaman” membuat UX drift dan sulit diaudit.  
**Migration**: gunakan event review eksplisit (mis. `ReviewRevertFiltered`) dari layer mapping input utama.

