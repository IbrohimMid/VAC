- [ ] E1: Tidak ada flow aktif yang bergantung pada `show_file_changes_popup` dan `file_changes_*`
- [ ] E1: `/review` dan Ctrl+G selalu membuka review workstation (surface tunggal)
- [ ] E1: Semua aksi review (open/close/filter/selection/diff/revert/editor) hanya hidup di workstation
- [ ] E1: `view.rs` tidak lagi merender legacy popup; hint/shortcut tidak menyebut legacy path

- [ ] E2: `vac autopilot up` tidak spawn `vac runtime start` sebagai mekanisme utama
- [ ] E2: Ada `AutopilotController` di `vac_runtime` dengan loop lifecycle (config, queue, state heartbeat, shutdown)
- [ ] E2: Mode monitor observable: tidak dequeue job; state polling/idle + last_event task_queued saat queue berisi
- [ ] E2: Mode auto observable: dequeue + execute job; state + last_event berubah nyata
- [ ] E2: `waiting_approval` bukan placeholder: bisa muncul dan tercatat di `.vac/autopilot.state`
- [ ] E2: `.vac/autopilot.state` ditulis oleh controller (ownership jelas)

- [ ] E3: Ada minimal 1 E2E test yang membuktikan mode monitor tidak mengeksekusi job
- [ ] E3: Ada minimal 1 E2E test yang membuktikan mode auto benar-benar mengeksekusi job
- [ ] E3: Ada test yang memunculkan state `waiting_approval`
- [ ] E3: Ada E2E test review workstation (revert selected/filtered/all + open editor tidak panic)
- [ ] E3: Ada lifecycle E2E test (up/status/down) dengan PID/state cleanup

- [ ] E4: `/fix` dan `/explain` tetap semantik (tidak literal) dan konsisten input bar + palette
- [ ] E4: Permission mode hint tidak drift setelah unifikasi review
- [ ] E4: `vac autopilot status --format json` punya assertion minimal field untuk audit mesin
- [ ] E4: Tidak ada mismatch hint vs event shortcut aktif

- [ ] Verifikasi: `cargo test --workspace` lulus
- [ ] Delivery: perubahan sudah ada di `main` dan laporan pasca-push mencantumkan commit hash + file berubah + per-stream selesai/partial + test yang dijalankan

