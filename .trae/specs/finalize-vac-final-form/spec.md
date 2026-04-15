# VAC Final Form Spec (Superbatch F)

## Why
VAC sudah mencapai Wave 5 PASS secara praktis, tetapi “final form operator product” membutuhkan plane yang menyatu dan operasional: approval, review, autopilot, recovery/audit, ACP, dan release gates. Final form tidak boleh dinyatakan PASS hanya karena surface feature ada.

## What Changes
- Membuat approval plane tunggal dan durable lintas `vac interactive` / `vac run` / `vac acp` / `vac autopilot` / runtime controller.
- Memfinalisasi review workstation menjadi operator review desk (diff engine matang, action model granular, export patch, approval-aware).
- Mengangkat autopilot controller menjadi pusat orkestrasi (state machine lengkap, source-of-work, approval integration, recovery).
- Mematangkan runtime/queue/recovery/audit (atomic write, corruption handling, execution journal, resume semantics, inspect richness).
- Memfinalisasi ACP sebagai external control plane (approve/reject, observe, inspect, streaming events, non-blocking).
- Memfinalisasi export/audit/forensics (format export nyata, audit bundle, redaction policy).
- Menyatukan vocabulary dan surface status/doctor lintas CLI/TUI.
- Menambah release hardening dan gates (integration + chaos tests).

## Impact
- Affected specs: Approval Plane, Review Plane, Autopilot Plane, Recovery Plane, Control Plane (ACP), Audit/Export, Operator UX, Release Gates.
- Affected code (indikatif):
  - `crates/vac_core/src/engine.rs`
  - `crates/vil_swarm/src/orchestrator.rs`
  - `crates/vac_core/src/acp.rs`
  - `crates/vac_core/src/approval_store.rs` (**baru**)
  - `crates/vac_runtime/src/autopilot.rs`
  - `crates/vac_runtime/src/scheduler.rs`
  - `crates/vac_runtime/src/queue.rs`
  - `crates/vac_runtime/src/journal.rs` (**baru**)
  - `crates/vac_cli/src/tui/runner.rs`
  - `crates/vac_cli/src/tui/view.rs`
  - `crates/vac_cli/src/tui/services/review.rs`
  - `crates/vac_cli/src/tui/services/review_export.rs` (**baru**)
  - `crates/vac_cli/src/commands/run.rs`
  - `crates/vac_cli/src/commands/acp.rs`
  - `crates/vac_cli/src/commands/export.rs`
  - Tests: `crates/*/tests/*`

## ADDED Requirements

### Requirement: Unified Approval State Machine (F1-A)
Sistem SHALL menyediakan state machine approval eksplisit yang dipakai lintas semua entrypoint dan runtime plane.

State minimum:
- `NoApprovalNeeded`
- `ApprovalPending { tool_call_id, tool_name, risk, reason, source }`
- `Approved`
- `Rejected`
- `Expired`
- `Cancelled`

#### Scenario: Pending Approval Is Durable
- **WHEN** tool-call nyata memerlukan approval
- **THEN** sistem menyimpan approval sebagai state durable (bukan event lepas) dan bisa di-load kembali setelah restart

#### Scenario: Approval Is Observable Everywhere
- **WHEN** approval pending ada
- **THEN** TUI / CLI / ACP / autopilot status menampilkan approval pending dengan vocabulary yang sama

### Requirement: Durable Approval Store (F1-B)
Sistem SHALL menyimpan approval ke `.vac/approvals/*.json` dan mendukung resume/restart.

#### Scenario: Resolve By Id
- **WHEN** operator resolve approval by `tool_call_id`
- **THEN** store ter-update dan eksekusi job lanjut atau fail sesuai keputusan, dengan jejak audit

#### Scenario: Expiration
- **WHEN** approval stale melewati TTL/policy
- **THEN** approval berpindah ke `Expired` dan tidak deadlock

### Requirement: Unified Risk Policy Vocabulary (F1-C)
Sistem SHALL memakai vocabulary permission/approval yang seragam lintas semua surface:
- `prompt`
- `auto_safe`
- `auto_all`
- `deny`

#### Scenario: No Drift
- **WHEN** operator memeriksa permission mode di TUI, CLI, ACP, dan autopilot status
- **THEN** label dan meaning konsisten (tidak ada boolean `auto_approve` yang berdiri sendiri tanpa mapping)

### Requirement: Real Waiting Approval for Real Tool-Calls (F1-D)
Sistem SHALL membuat autopilot masuk `WaitingApproval` saat tool-call nyata memerlukan approval, bukan hanya jalur synthetic.

#### Scenario: WaitingApproval Triggered by Real RunTask/PatchProposal
- **WHEN** job `RunTask/PatchProposal/AutoFixLowRisk` memicu tool approval
- **THEN** autopilot state menjadi `WaitingApproval { tool_call_id }` dan dapat di-resolve dari minimal dua channel (mis. TUI + ACP, atau file + ACP)

### Requirement: Review Workstation Final Form (F2)
`/review` SHALL membuka operator review desk yang matang.

#### Scenario: Diff Engine Robust
- **WHEN** membuka diff untuk file kecil/besar/binary/missing snapshot
- **THEN** sistem menampilkan state yang benar (viewport, lazy load, error state jelas, dan fallback untuk binary/large)

#### Scenario: Granular Actions
- **WHEN** operator melakukan revert/approve/reject/stage/export/mark-reviewed
- **THEN** status per-file konsisten dan jejak audit tercatat

#### Scenario: Export Patch
- **WHEN** operator export patch dari review
- **THEN** patch bundle valid dan dapat diaudit

### Requirement: Autopilot Controller Final Form (F3)
Autopilot SHALL menjadi controller daemon pusat orkestrasi dengan state machine lengkap, source-of-work, approval integration, backoff/retry, dan recovery.

State minimum:
- `Idle`, `Polling`, `Discovering`, `Planning`, `Queueing`
- `Executing { job_id }`
- `WaitingApproval { tool_call_id }`
- `Backoff { until }`
- `Paused`, `Degraded`, `Failed { error }`

Event minimum:
- `Heartbeat`, `TaskDiscovered`, `TaskQueued`, `TaskStarted`
- `ApprovalNeeded`, `ApprovalResolved`
- `TaskCompleted`, `TaskFailed`, `RetryScheduled`
- `Paused`, `Resumed`

#### Scenario: Mode Semantics Real
- **WHEN** mode `monitor`
- **THEN** hanya observe/discover tanpa eksekusi
- **WHEN** mode `auto_safe`
- **THEN** hanya mengeksekusi low-risk sesuai policy
- **WHEN** mode `auto`
- **THEN** mengeksekusi penuh dalam policy
- **WHEN** mode `paused`
- **THEN** tidak mengeksekusi tetapi heartbeat tetap jalan

#### Scenario: Recovery After Crash
- **WHEN** controller restart setelah crash
- **THEN** load queue + approvals + last state, recover/mark interrupted job dengan terminal status yang eksplisit

### Requirement: Runtime/Queue/Recovery Finalization (F4)
Runtime SHALL memiliki durable execution model yang bisa diaudit dan dipulihkan.

#### Scenario: Queue Durability
- **WHEN** terjadi power loss / kill -9 / corruption
- **THEN** queue recovery graceful (atomic write, versioned format, migration path)

#### Scenario: Execution Journal
- **WHEN** job berjalan
- **THEN** journal mencatat enqueue/start/approval pending/resolved/success/failure/cancel/retry

#### Scenario: Inspect Richness
- **WHEN** operator menjalankan `vac runtime inspect <id>`
- **THEN** terlihat history, approval history, retries, state transitions, summary, last error, artifacts

### Requirement: ACP External Control Plane Finalization (F5)
ACP SHALL mendukung submit/observe/resolve approval/inspect dan streaming events tanpa blocking serial path.

#### Scenario: Resolve Approval via ACP
- **WHEN** approval pending ada
- **THEN** ACP dapat `approve_tool`/`reject_tool` dan hasilnya menggerakkan eksekusi

#### Scenario: Streaming Events
- **WHEN** client subscribe
- **THEN** menerima event minimal: task started, approval needed/resolved, task done, error

### Requirement: Export/Audit/Forensics Finalization (F6)
Export SHALL bukan placeholder dan mendukung audit bundle.

#### Scenario: Audit Bundle
- **WHEN** operator export audit bundle
- **THEN** bundle memuat metadata session, task history, approvals, queue history, autopilot transitions, selected diffs, optional logs

#### Scenario: Redaction Policy
- **WHEN** mode export `full/safe/redacted`
- **THEN** redaction konsisten dan dapat diaudit

### Requirement: Operator UX/Product Surface Finalization (F7)
Operator SHALL memahami system state tanpa membaca kode.

#### Scenario: Unified Vocabulary
- **WHEN** operator melihat status CLI/TUI
- **THEN** vocabulary mode/approval/queue/runtime/autopilot/review/resume/restore/export konsisten

#### Scenario: Doctor Final
- **WHEN** `vac doctor` dijalankan
- **THEN** memeriksa queue health, approval store health, autopilot state health, snapshot consistency, config drift, editor config, ACP readiness

### Requirement: Release Hardening & Gates (F8)
Release SHALL diblok jika contract penting rusak (bukan compile-only).

#### Scenario: Integration Gates
- **WHEN** `cargo test --workspace`
- **THEN** ada coverage untuk: review workstation, runtime persistence, autopilot lifecycle + waiting approval real, ACP approval resolution, export formats, recovery after restart

#### Scenario: Chaos Tests
- **WHEN** failure injected (corrupt queue, stale pid, missing snapshot/editor, approval timeout, interrupted run)
- **THEN** sistem fail gracefully tanpa silent failure

## MODIFIED Requirements

### Requirement: Autopilot & Scheduler Relationship
Autopilot SHALL menjadi sumber kebenaran orkestrasi. Scheduler/runtime loop generik boleh tetap ada, tetapi tidak menjadi mekanisme utama autopilot plane.

## REMOVED Requirements

### Requirement: Approval As Ephemeral Events
**Reason**: event-only approval mudah drift, tidak durable, dan tidak bisa di-resume.  
**Migration**: pindahkan ke approval store + approval state machine tunggal.

