# Tasks

- [ ] Task 1 (Stream 1): Implement Patch D test suite untuk review workstation + built-in slash semantics
  - [ ] Tambah unit tests untuk `AppState` review helpers: projection/filter, selection normalization, open/close transitions
  - [ ] Tambah tests untuk revert state update: selected / filtered / all (menggunakan tempdir + snapshot)
  - [ ] Tambah tests untuk `load_diff` (snapshot vs working tree fallback)
  - [ ] Tambah test untuk `/review` (masuk workstation) dan `/fix`/`/explain` (BuiltInWithPrompt + args, bukan literal)

- [ ] Task 2 (Stream 1): Cleanup desain event review (hapus event “pinjaman”)
  - [ ] Pastikan `ReviewRevertFiltered` dimapping langsung dari layer input (keyboard)
  - [ ] Hapus semua penggunaan event non-review untuk aksi review (mis. hijack `ToggleSidePanel`)
  - [ ] Sinkronkan hints shortcut dengan mapping aktual

- [ ] Task 3 (Stream 1): Implement diff scroll end-to-end
  - [ ] Tambah event input untuk diff scroll (mis. `ReviewDiffScrollUp/Down` atau reuse `PageUp/PageDown` dengan mode-aware routing)
  - [ ] Update event loop untuk memutakhirkan `ReviewDiffState.scroll`
  - [ ] Update renderer workstation untuk menghormati `scroll` (render viewport slice, bukan seluruh diff)

- [ ] Task 4 (Stream 2): Review workstation polish (lazy reload diff, status/error visibility, editor fallback)
  - [ ] Lazy reload/refresh diff ketika selection berubah atau tampilkan indikator diff stale
  - [ ] Render status per file (pending/restored/failed + has_snapshot) secara konsisten
  - [ ] Tampilkan error restore per file dan error load diff di panel kanan (bukan hanya assistant message)
  - [ ] Open editor: `VAC_EDITOR` → `EDITOR` → fallback `nvim/vim/nano`, tanpa crash bila tidak ada editor
  - [ ] Pastikan `/review` end-to-end usable dengan keyboard hints yang benar

- [ ] Task 5 (Stream 3): Autopilot maturity phase 1 (state machine + state file + status + interval + mode)
  - [ ] Perkaya state machine runtime/autopilot: Idle, Polling, Executing{job_id}, WaitingApproval{tool_call_id?}, Backoff{until}, Failed{error}
  - [ ] Perkaya `.vac/autopilot.state` dengan field minimal yang disepakati + `updated_at`
  - [ ] Tambah event taxonomy minimal (TaskQueued/Started/ApprovalNeeded/Completed/Failed/RetryScheduled/StateChanged)
  - [ ] Update `vac autopilot status` untuk render field-field tersebut
  - [ ] Gunakan `poll_interval_secs` dalam loop nyata
  - [ ] Buat mode monitor vs auto observable beda perilaku (phase 1; minimal tapi audit-able)

- [ ] Task 6 (Stream 4): Hardening tambahan (runtime/queue + permission consistency + test gate)
  - [ ] Pastikan helper path queue dipakai konsisten dan tambah test restart/reload yang paling load-bearing
  - [ ] Audit dan rapikan messaging/hints permission mode agar tidak drift setelah perubahan
  - [ ] Tambah integration/unit test paling penting untuk autopilot/status/queue/permission drift

- [ ] Task 7: Verifikasi dan delivery ke `main`
  - [ ] Jalankan `cargo test --workspace`
  - [ ] Push langsung ke `main`
  - [ ] Buat laporan pasca-push: commit hash, daftar file berubah, per-stream selesai/partial, dan test yang dijalankan

# Task Dependencies
- Task 3 depends on Task 2
- Task 4 depends on Task 2 and Task 3
- Task 7 depends on Task 1-6

