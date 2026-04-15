- [x] Autopilot `JobKind::ToolCall` tidak lagi bypass approval plane (tidak ada `record_decision + route_approved` sebelum `ApprovalHandle`)
- [x] ToolCall request/decision dipersist ke `.vac/approvals` dengan `task_id + session_id`
- [x] ToolCall approval target di-register scoped (`task_id + session_id`) dan resolve lewat `ApprovalHandle`
- [x] Reject memblokir eksekusi tool
- [x] Stale/wrong-target menghasilkan error jelas dan tidak resolve record
- [x] Overlap routing terbukti tidak tertukar
- [x] `cargo test -p vac_core -p vac_runtime -p vac_cli` lulus

