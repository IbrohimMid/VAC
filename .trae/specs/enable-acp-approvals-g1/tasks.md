# Tasks

- [x] Task G1.1: Audit jalur ACP vs approval plane existing
  - [x] Verifikasi `drop(approval_tx)` (atau ekuivalen) di `crates/vac_cli/src/commands/acp.rs`
  - [x] Verifikasi `approve_tool` masih stub / “not supported” di `crates/vac_core/src/acp.rs`
  - [x] Verifikasi approval plane existing di `VacEngine` (mis. `ApprovalHandle`, `active_approval_tx`, `approve_tool_call`/`reject_tool_call`)

- [x] Task G1.2: Wire ACP `approve_tool`/reject ke approval plane engine (minimum patch)
  - [x] Tambah handler/callback resolve approval pada ACP server (tanpa framework baru)
  - [x] Implement routing `approve_tool` → engine.approve_tool_call / engine.reject_tool_call
  - [x] Support `reject_tool` sebagai alias (opsional)
  - [x] Pastikan error path jelas bila tidak ada approval aktif

- [x] Task G1.3: Hentikan auto-reject by design di `vac acp` CLI
  - [x] Hapus `drop(approval_tx)` (atau ubah wiring agar engine menyediakan `active_approval_tx` yang bisa di-resolve dari ACP)
  - [x] Pastikan message permission mode tidak misleading (ACP approval-capable)
  - [x] Pastikan task handler ACP mem-pass wiring yang dibutuhkan untuk approval handler

- [x] Task G1.4: Verifikasi & compile gates
  - [x] Jalankan `cargo test -p vac_core -p vac_cli` (atau `cargo test --workspace` bila feasible)
  - [x] Tambah minimal test bila sudah ada harness yang tepat (opsional; tidak membuat framework test baru)

- [x] Task G1.5: Commit perubahan terfokus
  - [x] Commit tunggal: “Enable ACP approvals (G1)”
  - [x] Pastikan diff kecil dan audit-friendly

# Task Dependencies
- Task G1.2 depends on Task G1.1
- Task G1.3 depends on Task G1.2
- Task G1.4 depends on Task G1.2 and Task G1.3
- Task G1.5 depends on Task G1.4
