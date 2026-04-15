# Re-enable Critical TUI UX (Phase 1) Spec

## Why
VAC saat ini menggunakan TUI shell yang di-fork dari Stakpak, namun sebagian besar fitur advanced masih non-aktif sehingga alur kerja user terasa “bare bones” dan tertinggal dibandingkan Stakpak. Fase 1 ini memulihkan komponen UX ber-ROI tinggi yang paling berdampak untuk trust, navigasi, dan kontrol runtime.

## What Changes
- Mengaktifkan kembali komponen TUI prioritas dari basis Stakpak (dengan adaptasi ke tipe VIL/VAC) untuk:
  - Approval UI yang lebih kaya (preview + aksi accept/reject yang jelas)
  - Model switcher interaktif tanpa restart
  - File search/fuzzy navigation
  - Toast/notifications untuk feedback aksi user
  - Changeset tracking + review popup untuk perubahan file
- Menambahkan adapter layer minimal untuk menjembatani perbedaan tipe dan event antara Stakpak TUI services dan VAC runtime (VIL engine, tool calls, approvals).
- Menstandarkan event/state contract yang dibutuhkan oleh services di atas agar tidak bergantung pada `stakpak_*` crates.

## Impact
- Affected specs: TUI Interaction, Approval Flow, Runtime/Execution Visibility, File Navigation
- Affected code: `crates/vac_cli` (TUI), state types (`app/types.rs` atau ekuivalen VAC), adapter layer TUI (`tui/adapter/`), services yang saat ini berada di `services_stakpak_disabled/`

## ADDED Requirements
### Requirement: Rich Approval Bar
Sistem SHALL menampilkan approval bar yang kaya (reason, scope, preview ringkas, dan aksi) ketika runtime meminta persetujuan user.

#### Scenario: Approval muncul dan menahan eksekusi
- **WHEN** runtime menghasilkan request approval untuk aksi berisiko (mis. write/edit file atau menjalankan perintah shell)
- **THEN** TUI menampilkan approval bar dengan opsi minimal accept/reject
- **AND THEN** eksekusi tertahan sampai user memberi keputusan

#### Scenario: Approval menampilkan preview perubahan
- **WHEN** approval terkait perubahan file atau changeset
- **THEN** TUI menampilkan ringkasan file yang terdampak dan preview diff ringkas (atau link/popup ke preview yang lebih detail)

### Requirement: Interactive Model Switcher
Sistem SHALL menyediakan model switcher interaktif yang memungkinkan user mengganti model aktif tanpa restart TUI.

#### Scenario: Switch model saat sesi berjalan
- **WHEN** user membuka model switcher dari shortcut/palette
- **THEN** TUI menampilkan daftar model terkonfigurasi (dengan grouping provider bila tersedia)
- **AND THEN** pemilihan model mengubah model aktif untuk request berikutnya dalam sesi yang sama

### Requirement: File Search & Navigation
Sistem SHALL menyediakan pencarian file fuzzy untuk memilih file dengan cepat dari workspace.

#### Scenario: Cari dan pilih file
- **WHEN** user mengetik query pada file search
- **THEN** TUI menampilkan kandidat file dengan ranking fuzzy
- **AND THEN** memilih hasil membuka preview yang relevan (minimal menampilkan path dan memungkinkan aksi lanjutan seperti view diff/changes)

### Requirement: Toast Notifications
Sistem SHALL menampilkan toast notification non-intrusif untuk aksi UI penting (success/error/info).

#### Scenario: Feedback untuk aksi user
- **WHEN** user melakukan aksi seperti mengganti model, menyetujui/menolak approval, atau membuka file search
- **THEN** TUI menampilkan toast singkat yang menjelaskan hasil aksi

### Requirement: Changeset Tracking & Review
Sistem SHALL melacak perubahan file sebagai changeset dan menyediakan UI untuk meninjau perubahan.

#### Scenario: Review changeset sebelum approval/commit
- **WHEN** runtime mengubah file atau menyiapkan patch
- **THEN** TUI menambahkan entri ke changeset aktif (file path + jenis perubahan)
- **AND THEN** user dapat membuka popup review changeset untuk melihat daftar perubahan dan preview diff

## MODIFIED Requirements
### Requirement: VAC TUI Runtime State
State TUI VAC yang sebelumnya minimal SHALL diperluas untuk mendukung: status approvals (pending/decided), model aktif, indeks file (untuk search), dan changeset aktif beserta metadata preview.

## REMOVED Requirements
Tidak ada.

