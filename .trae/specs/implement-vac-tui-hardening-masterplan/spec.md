# VAC TUI Hardening Masterplan Spec

## Why
VAC saat ini harus diposisikan sebagai **advanced beta / pre-production**, bukan near-production. Fondasi arsitektur dan engine VIL sudah kuat, tetapi operator surface, startup truthfulness, command contract, telemetry surfacing, release trust chain, dan production evidence belum cukup rapi untuk diposisikan sejajar dengan alat operator-grade lainnya. Masterplan ini bertujuan mengubah VAC menjadi control plane yang truthful, deterministic, dan operator-grade.

## What Changes
- **Wave 0**: Hentikan penambahan surface baru dan selesaikan hutang teknis (feature freeze).
- **Wave 1**: Pecah giant controller menjadi handler graph yang bisa dipelihara (TUI kernel refactor).
- **Wave 2**: Hilangkan phantom commands dan samakan semua jalur eksekusi command.
- **Wave 3**: Startup hydration yang jujur, hidup, dan menjelaskan capability.
- **Wave 4**: Message send, paste, image attachment, dan busy state menjadi deterministic dengan backpressure.
- **Wave 5**: Approval subsystem extraction menjadi state machine eksplisit.
- **Wave 6**: Shell mode menjadi subsystem nyata dengan lifecycle dan state diagram yang jelas.
- **Wave 7**: Runtime telemetry surfacing, UI memantulkan capability engine.
- **Wave 8**: Model/profile/rulebook switcher hardening dengan lifecycle lengkap.
- **Wave 9**: Changeset/review/editor unification ke dalam single changeset store.
- **Wave 10**: Workspace split untuk control-plane infra (mendekati Stakpak discipline).
- **Wave 11**: Testing and evidence hardening (command parity, popup interception, queue, approval, shell, dll).
- **Wave 12**: Product truthfulness polish setelah arsitektur selesai.
- **Track B (B1-B5)**: Paralel mutation gate fail-closed, release trust chain, operability enforcement, trace redaction, dan evidence gate.

## Impact
- **Affected specs**: TUI control-plane, trust/release/redaction gates, shell governance, telemetry engine.
- **Affected code**: `crates/vac_cli/src/tui/**`, serta penambahan crate baru seperti `crates/vac_approvals`, `crates/vac_shell`, `crates/vac_changeset`, `crates/vac_session_control`, `crates/vac_tui_runtime`.

## ADDED Requirements
### Requirement: Control-plane hardening
Sistem SHALL menyediakan TUI kernel yang ter-refactor secara modular, command system yang seragam, startup hydration status, input/output queue, serta pemisahan spesifik untuk shell runtime, approval, dan changeset store.
Sistem SHALL menerapkan operability enforcement (memory/disk quota), mutation gate fail-closed, trace redaction, dan release trust chain closure.

## MODIFIED Requirements
### Requirement: UI Monolith
`controller.rs` SHALL diubah menjadi façade tipis. Semua popup, input, dan shell context harus memiliki handler mandiri.
### Requirement: UI Surface Truthfulness
Sistem tidak boleh menampilkan state ambigu seperti `unknown` atau `Model: none`.
### Requirement: Approval System
Approval policy parsing tidak boleh dilakukan di giant controller, melainkan menggunakan domain types pada `crates/vac_approvals`.

## REMOVED Requirements
### Requirement: Feature Penambahan TUI Baru
**Reason**: Fokus plan adalah hardening, bukan menambah surface eksperimental.
**Migration**: Feature freeze diberlakukan hingga Wave 3 selesai. Backlog/spec lama harus dihapus.
