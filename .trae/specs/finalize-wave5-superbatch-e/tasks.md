# Tasks

- [ ] Task E1: Review Workstation Unification (hapus legacy path)
  - [ ] Hapus state legacy dari `AppState` (`show_file_changes_popup`, `file_changes_*`) dan migrasikan semua call-site
  - [ ] Hapus handler input/event-loop untuk legacy popup dan rute semua shortcut review ke workstation
  - [ ] Hapus renderer legacy popup dari `view.rs`
  - [ ] Pastikan `/review` dan Ctrl+G selalu menuju workstation, dan tidak ada logic revert/editor ganda

- [ ] Task E2: Autopilot Controller Phase 2 (controller nyata, bukan wrapper)
  - [ ] Tambah `AutopilotController` di `vac_runtime` (file baru `autopilot.rs`)
  - [ ] Controller loop: load config, open persistent queue, publish heartbeat state, handle shutdown
  - [ ] Mode behavior:
    - [ ] monitor: tidak dequeue/execute job, tapi update state + last_event untuk observability
    - [ ] auto: dequeue + execute job, update state machine dan event taxonomy
  - [ ] Waiting approval path:
    - [ ] Saat approval dibutuhkan, state file harus masuk `waiting_approval` dan last_event `approval_needed`
    - [ ] Pastikan kembali ke `executing/idle` setelah approval diproses
  - [ ] Update `vac autopilot up` agar menjalankan controller langsung (bukan `runtime start`)
  - [ ] Pastikan `.vac/autopilot.state` ditulis oleh controller

- [ ] Task E3: End-to-End Integration Proof
  - [ ] Review workstation E2E test:
    - [ ] `/review` membuka workstation
    - [ ] revert selected/filtered/all mengubah working tree + status item
    - [ ] open editor tidak panic (editor tersedia/tidak tersedia)
  - [ ] Autopilot monitor mode E2E test:
    - [ ] queue berisi job, controller monitor berjalan, job tetap queued
    - [ ] state file menunjukkan polling + last_event task_queued
  - [ ] Autopilot auto mode E2E test:
    - [ ] queue berisi job, controller auto berjalan, job bergerak status sampai selesai/gagal
    - [ ] state file menunjukkan transisi + last_event berubah
  - [ ] Waiting approval E2E test:
    - [ ] job/tool memerlukan approval, state file mencapai waiting_approval
  - [ ] Lifecycle E2E test:
    - [ ] autopilot up/status/down dan PID/state cleanup benar

- [ ] Task E4: Surface Cleanup & Gate Tightening
  - [ ] Pastikan `/fix` dan `/explain` tidak regress ke literal (input bar + palette)
  - [ ] Pastikan permission mode hint tidak drift setelah unifikasi review
  - [ ] Tambah assertion untuk autopilot status `--format json` (field minimal + kompatibilitas)
  - [ ] Rapikan hint/labels agar sinkron dengan shortcut & event yang benar-benar aktif

- [ ] Task E5: Verifikasi dan Delivery (superbatch besar)
  - [ ] Jalankan `cargo test --workspace`
  - [ ] Commit tunggal untuk Superbatch E (boleh squash)
  - [ ] Push langsung ke `main`
  - [ ] Laporan pasca-push: commit hash, daftar file berubah, per-stream selesai/partial, dan output test yang dijalankan

# Task Dependencies
- Task E3 depends on Task E1 and Task E2
- Task E4 depends on Task E1 and Task E2
- Task E5 depends on Task E1-E4

