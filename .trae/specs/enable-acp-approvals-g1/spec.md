# Enable ACP Approvals (G1) Spec

## Why
Jalur `vac acp` saat ini menutup approval plane secara sengaja (auto-reject) sehingga ACP tidak bisa mencapai parity dengan jalur `run`/`tui` untuk approval tool-call.

## What Changes
- Hubungkan method ACP `approve_tool`/`reject_tool` ke approval plane yang sudah ada di `VacEngine` (via `ApprovalHandle`/`active_approval_tx`), bukan stub “not supported”.
- Hentikan perilaku auto-reject by design di `vac_cli::commands::acp` (jangan menutup channel approval).
- Pastikan ACP server menerima handler untuk resolve approval agar `approve_tool` dapat men-trigger approve/reject aktual pada task yang sedang berjalan.
- **Non-goals**: membuat approval framework baru, mengubah semantik VIL/Swarm, refactor besar engine/planner, atau memperluas protocol ACP di luar minimum yang dibutuhkan.

## Impact
- Affected specs: ACP control plane, Approval parity lintas entrypoint.
- Affected code:
  - `crates/vac_cli/src/commands/acp.rs`
  - `crates/vac_core/src/acp.rs`
  - (opsional, bila perlu untuk compile) signature/handler wiring terkait ACP start.

## ADDED Requirements

### Requirement: ACP Approval Resolution (G1)
Sistem SHALL memungkinkan client ACP meng-approve atau me-reject tool-call yang sedang menunggu approval, menggunakan approval plane yang sama dengan jalur `run`/`tui`.

#### Scenario: Approval Required → Approve (Success)
- **WHEN** task yang dijalankan via ACP memicu event `approval_required`
- **AND WHEN** client mengirim `approve_tool` dengan `tool_call_id`
- **THEN** keputusan approval dikirim ke engine (via `ApprovalHandle`/`active_approval_tx`)
- **AND THEN** eksekusi task lanjut (tidak auto-reject hanya karena channel ditutup)

#### Scenario: Approval Required → Reject (Success)
- **WHEN** task yang dijalankan via ACP memicu event `approval_required`
- **AND WHEN** client mengirim `approve_tool` dengan `approved=false` (atau `reject_tool` bila tersedia) dan optional `feedback/reason`
- **THEN** keputusan reject dikirim ke engine dan task menangani penolakan sesuai semantik existing (tanpa perubahan semantics VIL)

#### Scenario: No Active Approval (Failure)
- **WHEN** client memanggil `approve_tool` tetapi tidak ada tool-call pending
- **THEN** ACP mengembalikan error yang jelas (mis. “No active task requires approval” dari `VacEngine::ApprovalHandle`) tanpa crash

## MODIFIED Requirements

### Requirement: ACP `approve_tool` Is Not a Stub
ACP `approve_tool` SHALL tidak lagi mengembalikan “not supported” dan MUST merutekan keputusan ke engine approval plane.

## REMOVED Requirements

### Requirement: ACP Auto-Reject By Closed Channel
**Reason**: menutup channel approval (`drop(approval_tx)`) membuat `approval_required` tidak dapat ditindaklanjuti dan memutus parity dengan jalur `run`/`tui`.  
**Migration**: ACP menyediakan handler resolve approval yang memanggil approval plane engine, tanpa menambah sistem approval baru.

