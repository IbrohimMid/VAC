# Tasks

- [x] Task G2.1: Implement approval registry scoped per task/session
  - [x] Simpan active approval sender per `task_id` dengan `session_id` untuk verifikasi
  - [x] Auto-cleanup saat channel closed

- [x] Task G2.2: Harden approve/reject resolution
  - [x] Validasi record approval eksis untuk `tool_call_id`
  - [x] Validasi state masih `pending`
  - [x] Validasi `task_id`/`session_id` cocok dengan task aktif di registry
  - [x] Jangan record_decision bila target stale/wrong

- [x] Task G2.3: Reject parity + auditability
  - [x] Pastikan `reject_tool` mengembalikan ack yang simetris (event `tool_approval_resolved`)
  - [x] Reason feedback diteruskan ke ack bila tersedia

- [x] Task G2.4: ACP protocol integration tests
  - [x] Approve flow
  - [x] Reject flow
  - [x] Stale/wrong-target approve must error + state tidak berubah
  - [x] Overlap flow: dua pending approvals tidak tertukar

- [x] Task G2.5: Gate
  - [x] `cargo test -p vac_core -p vac_cli`

