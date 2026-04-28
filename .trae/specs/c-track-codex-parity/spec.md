# C-TRACK — Codex-Grade VAC TUI Parity Spec

## Why
VAC memerlukan TUI dan CLI command grammar yang setara dengan Codex CLI untuk memberikan pengalaman pengguna yang lebih baik, tanpa mengorbankan VIL-native semantic execution plane dan arsitektur cockpit Stakpak-grade yang sudah ada. Tujuannya adalah menjadikan VAC sekelas Codex dalam hal ergonomi CLI/TUI dengan mendonor UX/runtime mechanics dari Codex secara brutal namun aman.

## What Changes
- Menambahkan alias command CLI yang setara Codex (`vac chat`, `vac exec`, `vac sandbox`).
- Mengimplementasikan fitur `/init` pada TUI default (`vac_tui_runtime`).
- Normalisasi UX Sandbox ke dalam `UserSandboxMode` (ReadOnly, WorkspaceWrite, DangerFullAccess).
- Pembuatan layer dispatcher event bergaya `AppEvent` di TUI.
- Pembuatan layer History/Transcript Cell yang terstruktur (UserMessage, AssistantStream, ToolCall, dll).
- Audit dan perbaikan streaming fidelity untuk feedback instan di UI.
- Implementasi *hard gate* untuk Plan Mode (membatasi tool destruktif).
- Penambahan Todo checklist pada conversation lane.
- Peningkatan UX Resume dan Fork session.
- Pemolesan overlay Diff dan Review.
- Visibilitas dan konfigurasi MCP inventory dan approval mode.
- Dokumentasi hook notifikasi (`turn_finished`, dll).

## Impact
- Affected specs: UX/UI TUI, CLI command grammar, Sandbox/Isolation Policy, Transcript Rendering.
- Affected code: `crates/vac_cli`, `crates/vac_tui_runtime`, `crates/vac_shell_app`, dokumentasi di `docs/`.

## ADDED Requirements
### Requirement: Codex-grade CLI grammar
Sistem HARUS menyediakan alias command baru seperti `vac exec`, `vac chat`, `vac sandbox` yang memetakan ke fungsionalitas yang ada tanpa menambah engine baru.

### Requirement: TUI `/init` parity
TUI default HARUS memiliki fitur `/init` yang memandu first-run checklist (Model, Sandbox, Sessions, Doctor, MCP, Status, Logs).

### Requirement: Sandbox UX Normalization
Sistem HARUS memiliki top-level sandbox mode (`ReadOnly`, `WorkspaceWrite`, `DangerFullAccess`) yang terhubung ke isolation/trust engine saat ini.

### Requirement: AppEvent dispatcher & Transcript Cells
TUI HARUS mengadopsi layer dispatcher berbasis `VacAppEvent` dan model `VacHistoryCell` secara parsial untuk meningkatkan keterbacaan history dan struktur event.

## MODIFIED Requirements
### Requirement: Plan Mode Gate
Plan Mode aktif HARUS secara otomatis memblokir operasi destruktif (file write, shell) hingga disetujui (hard gate).

### Requirement: Streaming Fidelity
TUI HARUS menampilkan output stream (teks dan tool calls) secara inkremental dan instan, bukan dirangkum di akhir.
