# ACP Protocol Hardening (G2) Spec

## Why
Jalur ACP sudah mencapai parity dasar untuk approval (G1), tetapi masih ada gap hardening yang membuat approval routing rawan salah target (stale/overlap) dan belum ada test protocol e2e yang benar-benar memukul jalur ACP.

## What Changes
- Ganti model “active approval handle global” menjadi registry scoped per task (task_id + session_id).
- Approval resolution (`approve_tool`/`reject_tool`) memverifikasi:
  - approval record eksis untuk `tool_call_id`
  - state masih `pending`
  - `task_id` + `session_id` dari record cocok dengan task aktif di registry
- Reject parity: `reject_tool` menghasilkan outcome simetris dengan approve (tool tidak dieksekusi, approval state resolved, ack ACP jelas).
- Cleanup: registry approval task dibersihkan otomatis saat receiver task selesai (channel closed), mencegah stale routing.
- Tambah integration test ACP protocol e2e: approve, reject, stale/wrong-target, overlap routing.

## Requirements

### Requirement: Task/Session Scoped Approval Routing
- **WHEN** ACP menerima `approve_tool`/`reject_tool` untuk `tool_call_id`
- **THEN** keputusan hanya boleh dirutekan ke task aktif yang benar (dibuktikan via `ApprovalStore` record `task_id`/`session_id`)
- **AND** bila tidak cocok / task sudah selesai / record tidak ada / state bukan pending: return error jelas dan tidak memutasi state.

### Requirement: Reject Parity End-to-End
- **WHEN** ACP menerima `reject_tool`
- **THEN** tool yang menunggu approval tidak dieksekusi, dan task melanjutkan dengan outcome “rejected” yang observable.

### Requirement: Cleanup Stale Approval Handles
- **WHEN** task selesai / approval channel ditutup
- **THEN** registry approval untuk task tersebut dibersihkan agar approval untuk task lama tidak bisa nyasar ke task berikutnya.

### Requirement: ACP Protocol E2E Tests
Sistem SHALL memiliki test yang memukul ACP server (newline-delimited JSON over TCP) untuk:
- approve flow
- reject flow
- stale/wrong-target flow
- overlap flow (dua pending approvals, routing tidak tertukar)

