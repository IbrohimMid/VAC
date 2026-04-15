# Unified ToolCall Approval (G4) Spec

## Why
Setelah G3, approval store sudah unified, tetapi `JobKind::ToolCall` non-engine di autopilot masih bypass approval plane substantif (langsung `record_decision + route_approved`). Ini membuat approval semantics berbeda untuk ToolCall dibanding engine-backed flow.

## Goal
ToolCall non-engine harus memakai approval plane yang sama secara substantif:
- request/decision di `.vac/approvals`
- scoped by `task_id + session_id`
- resolve via `ApprovalHandle` (registry + store validation)
- stale/wrong-target/already-resolved behavior identik

## Changes
- `JobKind::ToolCall` autopilot:
  - membuat `ApprovalStore::record_request` dengan `task_id=job.id` dan `session_id` session terbaru
  - register approval target via `ActiveApprovalRegistry` untuk `task_id + session_id`
  - menunggu input headless (`ApprovalIntent`) lalu resolve lewat `ApprovalHandle::{approve,reject}` (tidak lagi `record_decision` manual)
  - eksekusi tool hanya setelah approve sukses; reject tidak mengeksekusi tool
- Tambah tests autopilot ToolCall: approve, reject, stale, wrong-target, overlap routing.

