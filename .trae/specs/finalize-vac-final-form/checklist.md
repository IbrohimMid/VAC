- [ ] F1: Approval state machine tunggal ada dan dipakai lintas entrypoint (interactive/run/acp/autopilot/runtime)
- [ ] F1: Durable approval store `.vac/approvals/*.json` berfungsi (save/load/resolve/expire) dan survive restart
- [ ] F1: Vocabulary risk policy seragam `prompt/auto_safe/auto_all/deny` dan semua surface menampilkan mode yang sama
- [ ] F1: Autopilot `WaitingApproval` bisa muncul dari tool-call nyata (RunTask/PatchProposal/AutoFixLowRisk), bukan hanya synthetic path
- [ ] F1: Approval bisa diresolve minimal dari 2 channel berbeda dan tidak deadlock/hang

- [ ] F2: `/review` membuka workstation final yang robust (lazy diff, viewport, error states, large/binary handling)
- [ ] F2: Review actions granular bekerja (revert/approve/reject/stage/export/mark reviewed) dan status per file konsisten
- [ ] F2: Editor integration aman (resolve editor + suspend/restore) tanpa state corruption
- [ ] F2: Export patch dari review valid dan dapat diaudit

- [ ] F3: Autopilot controller state machine lengkap + event taxonomy final dipersist ke `.vac/autopilot.state`
- [ ] F3: Source-of-work minimal berfungsi (queue/cron/watch/manual/acp)
- [ ] F3: Mode semantics real (monitor/auto_safe/auto/paused) observable beda perilaku
- [ ] F3: Backoff/retry/drain + graceful shutdown bekerja
- [ ] F3: Recovery after crash tidak kehilangan konteks penting (queue+approvals+last state) dan interrupted job punya terminal status eksplisit

- [ ] F4: Queue durability hardening: atomic write + corruption handling + versioning + migration
- [ ] F4: Execution journal tersedia dan mencatat lifecycle job termasuk approval pending/resolved
- [ ] F4: `vac runtime inspect <id>` kaya (history, approval history, retries, transitions, artifacts, last error)
- [ ] F4: Chaos tests untuk corruption/kill/missing snapshot menunjukkan behavior graceful

- [ ] F5: ACP dapat resolve approval real (approve/reject/list pending) dan dapat inspect job/queue/autopilot
- [ ] F5: ACP streaming events tersedia dan non-blocking saat task panjang

- [ ] F6: Export formats nyata (vac-cbor, claude-jsonl, opencode-json, audit-json, patch-bundle)
- [ ] F6: Audit bundle memuat metadata + approvals + queue history + autopilot transitions + diffs + optional logs
- [ ] F6: Redaction policy (full/safe/redacted) konsisten dan diuji

- [ ] F7: Unified vocabulary dan status surfaces konsisten (vac status/runtime status/autopilot status/TUI status line)
- [ ] F7: `vac doctor` final mendeteksi drift penting (queue/approval store/autopilot state/snapshot/config/editor/acp readiness)

- [ ] F8: Release gates ada (integration + chaos + perf sanity) dan tidak ada placeholder/fake states
- [ ] Verifikasi: `cargo test --workspace` lulus
- [ ] Delivery: push ke `main` + laporan lengkap (commit hash, file berubah, per-stream selesai/partial, dan test dijalankan)

