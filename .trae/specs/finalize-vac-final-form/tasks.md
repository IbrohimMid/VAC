# Tasks

- [ ] Task F1: Approval Plane Finalization (state machine + durable store + unified policy)
  - [ ] Definisikan approval state machine tunggal (core types + lifecycle) dan integrasikan ke engine/orchestrator/runtime/entrypoints
  - [ ] Implement durable approval store `.vac/approvals/*.json` (save/load/resolve/expire) + migration untuk pending approvals lama bila ada
  - [ ] Unifikasi vocabulary risk policy `prompt/auto_safe/auto_all/deny` dan mapping dari semua flags/booleans yang ada
  - [ ] Implement real `WaitingApproval` autopilot untuk tool-call nyata (bukan hanya synthetic), dan pastikan bisa di-resolve minimal dari 2 channel
  - [ ] Tambah tests: approval persistence across restart + resolve from multiple channels + no deadlock

- [ ] Task F3: Autopilot Controller Final Form (state machine lengkap + source-of-work + recovery)
  - [ ] Perluas controller state machine + event taxonomy final dan persist ke `.vac/autopilot.state`
  - [ ] Implement source-of-work minimal: persistent queue + cron + file watch + manual submit + ACP-triggered
  - [ ] Implement mode semantics real: monitor/auto_safe/auto/paused
  - [ ] Implement backoff/retry/drain + graceful shutdown
  - [ ] Implement recovery after crash: load queue+approvals+last state, recover/mark interrupted jobs
  - [ ] Tambah E2E tests: lifecycle up/status/down, monitor vs auto_safe vs auto, waiting_approval real, restart recovery

- [ ] Task F4: Runtime/Queue/Recovery Finalization (durability + journal + inspect)
  - [ ] Queue hardening: atomic write, corruption handling, versioned format, migration path
  - [ ] Tambah execution journal (enqueue/start/approval pending/resolved/success/failure/cancel/retry) dan persist artifacts minimal
  - [ ] Definisikan resume semantics: resumed/restarted/abandoned/cancelled by recovery policy
  - [ ] Perkaya `vac runtime inspect <id>`: history + approval history + retries + transitions + artifacts
  - [ ] Tambah chaos tests: corruption + kill -9 simulation (as feasible) + missing snapshot

- [ ] Task F2: Review Workstation Final Form (diff engine + actions + export)
  - [ ] Diff engine robust: viewport per-file, lazy load, binary/large handling, error states jelas, optional syntax hint
  - [ ] Action model granular: revert/approve/reject/stage/export/mark reviewed
  - [ ] Review item state kaya: Pending/Viewed/Edited/Restored/Approved/Rejected/Failed
  - [ ] Editor integration: resolve editor + suspend/restore TUI aman tanpa state corruption
  - [ ] Implement review export service: patch bundle + summary review
  - [ ] Tambah tests: multi-file review flow, diff loader edge cases, editor fallback non-panic, export validity

- [ ] Task F5: ACP / External Control Plane Finalization (approve/inspect/stream)
  - [ ] Tambah ACP approval API: approve_tool, reject_tool, list_pending_approvals, inspect_job, resume_job, pause_autopilot, resume_autopilot
  - [ ] Tambah streaming events (non-blocking): task started, approval needed/resolved, done, error
  - [ ] Pastikan handler tidak serial blocking saat task panjang (async model)
  - [ ] Tambah integration tests ACP: resolve approval real + observe runtime + long-running does not hang

- [ ] Task F6: Export / Audit / Forensics Final Form (formats + bundle + redaction)
  - [ ] Implement export formats: vac-cbor, claude-jsonl, opencode-json, audit-json, patch-bundle
  - [ ] Implement audit bundle: session/task/approval/queue/autopilot transitions/diffs/logs
  - [ ] Implement redaction policy: full/safe/redacted + tests konsistensi redaction

- [ ] Task F7: Operator UX / Product Surface Finalization (vocabulary + doctor + status)
  - [ ] Unifikasi vocabulary dan copy/hints untuk mode/approval/queue/runtime/autopilot/review/resume/restore/export
  - [ ] Doctor final: queue health, approval store health, autopilot state, snapshot consistency, config drift, editor config, ACP readiness
  - [ ] Status surfaces konsisten: vac status, vac runtime status, vac autopilot status, TUI status line
  - [ ] Tambah tests: doctor checks critical drift + status json contract minimal

- [ ] Task F8: Release Hardening / Gates (matrix + chaos + perf sanity)
  - [ ] Tambah test matrix final (integration + contracts) dan pastikan tidak ada placeholder/fake states
  - [ ] Tambah chaos/failure tests minimal: queue corruption, stale pid, missing snapshot, missing editor, approval timeout, interrupted run
  - [ ] Tambah performance sanity: review pada banyak file, queue pada banyak job, autopilot heartbeat stability (bounded)

- [ ] Task F9: Verifikasi & Delivery
  - [ ] `cargo test --workspace`
  - [ ] Pastikan semua checklist terpenuhi dan tidak ada known silent failure path
  - [ ] Push ke `main` + laporan (commit hash, file berubah, per-stream selesai/partial, dan test dijalankan)

# Task Dependencies
- Task F1 depends on none (must be first)
- Task F3 depends on F1
- Task F4 depends on F1 and F3
- Task F2 depends on F1 (approval-aware actions) and F4 (snapshot/journal contracts)
- Task F5 depends on F1 and F3
- Task F6 depends on F1 and F4
- Task F7 depends on F1-F6
- Task F8 depends on F1-F7
- Task F9 depends on F1-F8

