# Tasks

- [x] Task G3.1: Audit approval plane lintas entrypoint dan identifikasi dual-store autopilot
- [x] Task G3.2: Migrasi autopilot ke approval store tunggal
  - [x] Tambah `ApprovalIntent` + `record_intent`/`clear_intent` di `ApprovalStore`
  - [x] Ubah autopilot untuk menunggu intent pada record store dan resolve via `ApprovalHandle`
  - [x] Hapus ekspor/kontrak approval file plane autopilot
- [x] Task G3.3: Samakan approve/reject TUI ke `ApprovalHandle`
- [x] Task G3.4: Pastikan resume path juga mem-persist approval request ke store
- [x] Task G3.5: Update test autopilot e2e untuk unblocking via store intent
- [x] Task G3.6: Gate `cargo test -p vac_core -p vac_cli -p vac_runtime`

