# Mature TUI Operator Path Spec

## Why
Berdasarkan analisis terbaru, VAC saat ini sudah sangat kuat secara arsitektural (VIL-native semantic specialization, control-plane hardening, checkpoint/runtime state, dan ACP server path). Namun, posisi produknya masih tertinggal dibandingkan dengan standar TUI CLI coding agent lain (seperti Stakpak atau OpenCode). TUI aktif saat ini terlalu tipis (hanya meng-handle `UserMessage -> eng.run_task()`). VAC perlu mengembangkan TUI yang jauh lebih mature (mendukung live approval, manajemen sesi, restore-first, real-time streaming, panel status/runtime, command palette) serta merapikan *public product story* agar posisinya sebagai "VIL-native autonomous development engine" semakin kokoh.

## What Changes
- Menulis ulang dan memperkaya README.md untuk mencerminkan kapabilitas engine internal VAC, memposisikan VAC sebagai *VIL-native autonomous development engine*.
- Memperluas runner TUI aktif (`vac_cli::tui::run_vac_tui`) dengan arsitektur event loop yang mampu menangani interaksi asinkron kompleks.
- Menambahkan kapabilitas TUI yang matang, termasuk live approval dialog, session management, restore-first mekanik, streaming interaktif, panel status/runtime (active tool calls, pending approvals), dan command palette.

## Impact
- Affected specs: TUI Interaction, Agent Session Management
- Affected code: `README.md`, `crates/vac_cli/src/tui.rs`, dan modul TUI terkait di dalam `vac_cli`

## ADDED Requirements
### Requirement: Full-featured TUI Operator
Sistem SHALL menyediakan antarmuka TUI yang mendukung live approval, manajemen sesi, real-time progress streaming, status panel untuk runtime, dan command palette.

#### Scenario: Live Approval
- **WHEN** agent membutuhkan persetujuan (approval) untuk mengeksekusi aksi berisiko tinggi (misalnya modifikasi file krusial)
- **THEN** TUI akan menampilkan dialog persetujuan (accept/reject) kepada user secara interaktif dan menahan eksekusi hingga mendapat respons.

### Requirement: Product Story (README) Update
Sistem SHALL memiliki dokumentasi README yang komprehensif.

#### Scenario: Menampilkan kapabilitas VAC
- **WHEN** user atau kontributor membaca `README.md`
- **THEN** mereka disajikan penjelasan terperinci mengenai arsitektur VIL-native, policy/privacy hardening, checkpoint run-state, kapabilitas ACP server, dan posisi VAC sebagai autonomous development engine.

## MODIFIED Requirements
### Requirement: `vac interactive` Command
Command `vac interactive` akan dirombak untuk menggunakan arsitektur event loop TUI yang baru. TUI yang diperbarui tidak hanya menjadi wrapper sederhana, melainkan mendukung berbagai panel interaktif dan komunikasi asinkron yang kaya dengan VAC Engine.
