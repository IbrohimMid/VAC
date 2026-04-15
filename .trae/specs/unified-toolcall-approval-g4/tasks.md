# Tasks

- [x] Audit main: ToolCall non-engine autopilot masih bypass approval plane
- [x] Implement unify ToolCall approval:
  - [x] Record request memakai `ApprovalStore` dengan `task_id + session_id`
  - [x] Register scoped target via `ActiveApprovalRegistry`
  - [x] Resolve via `ApprovalHandle` (approve/reject) sebelum eksekusi tool
- [x] Tests:
  - [x] Autopilot ToolCall approve e2e: tool benar-benar jalan
  - [x] Autopilot ToolCall reject e2e: tool tidak jalan
  - [x] Stale approval: approve error + record tetap pending
  - [x] Wrong-target isolation: mismatch task_id error + record tetap pending
  - [x] Overlap routing: dua approval aktif tidak tertukar
- [x] Gate: `cargo test -p vac_core -p vac_runtime -p vac_cli`

