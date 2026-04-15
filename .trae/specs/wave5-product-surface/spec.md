# Wave 5 Product Surface Spec

## Why
Untuk mengubah fondasi Wave 4 menjadi produk operator yang matang. Tujuannya adalah menjadikan VAC operator-friendly, scriptable, reviewable, runtime-capable, dan jujur secara produk tanpa placeholder command yang misleading. Fokus utama adalah pada "Product Surface & Operator Ergonomics" tanpa mengejar kapabilitas arsitektur level berikutnya (seperti editor bridge, enterprise memory, dll).

## What Changes
- Membangun Unified Command Surface (slash commands, command palette, project commands).
- Menambahkan Review Session Mode di TUI dengan bulk actions dan file-by-file review.
- Memperkenalkan permission modes resmi (default, ask, auto-approve-safe, auto-approve-all, dll) dan explainability.
- Menjadikan Runtime operasional dengan persistent job queue dan command inspeksi/pembatalan.
- Menaikkan Autopilot menjadi orchestration layer dengan mode, task source, dan state yang sesungguhnya.
- Memperluas Doctor menjadi entrypoint diagnostik dengan kapabilitas JSON, strict mode, dan autofix.
- Menambahkan kapabilitas Structured Output (JSON) di berbagai command CLI untuk keperluan automasi.
- Membangun testing matrix baru, gates, dan kesiapan rilis.

## Impact
- Affected specs: Command surface, TUI workflows, Runtime scheduler, Autopilot loop, Diagnostic tools.
- Affected code: `vac_cli` (main, commands, tui), `vac_runtime`, `vac_tools`, `vac_core`.

## ADDED Requirements
### Requirement: W5.1 Unified Command Surface
The system SHALL provide slash commands di TUI, project commands berbasis file, dan command palette.

### Requirement: W5.2 Review Mode & Change Management UX
The system SHALL provide a review-first workflow di TUI dengan daftar file yang berubah, bulk approve/reject, dan restore per-file.

### Requirement: W5.3 Approval & Permission UX
The system SHALL support mode permission eksplisit, menjelaskan alasan allowance/denial, dan menyajikan approval queue view.

### Requirement: W5.4 Runtime Productization
The system SHALL maintain a persistent job queue, mendukung scheduled jobs, dan menggantikan placeholder runtime command dengan operasi nyata.

### Requirement: W5.5 Autopilot
The system SHALL implement a real state machine untuk autopilot dengan actionable status dan event taxonomy.

### Requirement: W5.6 Doctor & Diagnostics 2.0
The system SHALL output diagnostics in JSON, support strict checking, and provide autofix for safe issues.

### Requirement: W5.7 Structured Output & Automation Surface
The system SHALL provide `--format json` flag untuk command status, doctor, runtime, dan autopilot.

### Requirement: W5.8 Testing & Gates
The system SHALL execute integration test untuk setiap stream besar sebelum rilis.

## MODIFIED Requirements
### Requirement: Existing CLI Commands
CLI commands seperti `runtime jobs` dan `autopilot` akan dimodifikasi dari placeholder/wrapper sederhana menjadi command yang fungsional secara stateful.
