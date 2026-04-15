- [x] Satu approval store durable (`.vac/approvals`) dipakai lintas ACP/run/TUI/autopilot
- [x] Autopilot tidak lagi memakai approval file plane terpisah sebagai source-of-truth
- [x] Approve/reject autopilot memakai state transition + routing validation yang sama dengan ACP (via `ApprovalHandle`)
- [x] Error semantics missing/stale/already-resolved konsisten (tidak silently succeed)
- [x] Tests parity lintas entrypoint (ACP e2e tetap lulus, autopilot e2e pakai store intent)
- [x] `cargo test -p vac_core -p vac_cli -p vac_runtime` lulus

