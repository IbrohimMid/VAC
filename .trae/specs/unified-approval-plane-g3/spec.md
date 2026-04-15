# Unified Approval Plane (G3) Spec

## Why
Approval sudah berfungsi di ACP (G1/G2), tetapi autopilot masih punya approval “file plane” terpisah yang berbeda semantik dari approval store core. Ini membuat approval terasa sebagai beberapa jalur yang mirip tapi bukan satu plane operasional.

## Goals
- Satu kontrak approval record yang konsisten lintas entrypoint: `vac run`, TUI, ACP, autopilot.
- Satu approval store durable (`.vac/approvals/*.json`) sebagai source of truth.
- Autopilot parity terhadap approval plane: waiting/decision memakai store + state transition yang sama.
- Error semantics konsisten: missing/stale/wrong-target/already-resolved tidak silently succeed dan tidak merusak task lain.

## What Changed
- Tambah `ApprovalIntent` sebagai mekanisme “headless approval input” berbasis store untuk autopilot: autopilot menunggu `intent` di record yang sama, lalu mengeksekusi `ApprovalHandle::approve/reject` agar routing + validation sama dengan ACP/TUI.
- Hapus ekspor approval file plane autopilot (`approval_file_path`, `*.autopilot.approvals`) sehingga tidak ada dual-store semantik.
- TUI approve/reject sekarang memakai `ApprovalHandle` (store+registry) alih-alih mengirim langsung ke channel approval_rx internal.
- `resume_run_state` juga mem-persist approval request ke store untuk parity dengan run_task path.

## Requirements

### Requirement: Single Durable Store
- Semua approval request/decision menggunakan `ApprovalStore` (record_request/record_decision) pada `.vac/approvals`.

### Requirement: Headless Intent for Autopilot
- Autopilot menulis keputusan approval sebagai `ApprovalIntent` pada record pending (`record_intent`) lalu memanggil `ApprovalHandle::{approve,reject}`.

### Requirement: Consistent Error Semantics
- Missing record, already resolved, stale/wrong-target harus error jelas dan tidak memutasi record menjadi resolved.

### Requirement: Tests
- Autopilot approval e2e membuktikan unblocking via `ApprovalStore::record_intent` (bukan file plane).
- ACP protocol e2e tetap lulus.
- `cargo test -p vac_core -p vac_cli -p vac_runtime` lulus.

