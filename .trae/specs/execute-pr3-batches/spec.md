# Execute PR-3 Batches Spec

## Why
Menyelesaikan dan membuktikan fungsionalitas dari PR-3A (Privacy Hardening) melalui sekumpulan pengujian (tests) dan pembaruan dokumentasi yang terstruktur. Ini memastikan kebenaran alur persetujuan, isolasi *subagent*, integrasi privasi antara *engine* dan *swarm*, serta menjaga agar repositori tetap bersih dan jujur terkait kemampuannya.

## What Changes
- Memperbarui `README.md` untuk merefleksikan arsitektur VAC sebagai VIL-native autonomous coding agent.
- Menambahkan pengujian (test) pada `vil_swarm::run_state` untuk memastikan `approved_tools` tersimpan pada *checkpoint*.
- Menambahkan pengujian pada `vil_swarm::subagent` untuk membuktikan penggunaan *shared privacy vault*.
- Menambahkan pengujian pada `vac_tools::router` untuk jalur penolakan (deny path) pada *sandboxed subagent*.
- Menambahkan pengujian pada `vac_tools::router` untuk membuktikan siklus *restore* dan *substitute* privasi pada eksekusi *tool*.
- Menambahkan *smoke test* pada `vac_core::engine` untuk memastikan *engine* menyuntikkan *shared privacy vault* ke *swarm*.
- Membersihkan *warning* kompilator/linter pada file yang terdampak PR-3A.
- Menambahkan dokumentasi arsitektur keamanan dan privasi yang baru di dalam direktori `docs/`.

## Impact
- Affected specs: Pengujian keamanan, alur kerja subagent, dan integrasi privasi.
- Affected code:
  - `README.md`
  - `crates/vil_swarm/src/run_state.rs`
  - `crates/vil_swarm/src/subagent.rs`
  - `crates/vac_tools/src/router.rs`
  - `crates/vac_core/src/engine.rs`
  - `crates/vac_core/src/security/secret_substitution.rs`
  - `crates/vil_swarm/src/orchestrator.rs`
  - `crates/vil_swarm/src/redaction.rs`
  - `docs/privacy_architecture.md` (file baru)

## ADDED Requirements
### Requirement: Validasi Keamanan dan Privasi Terdistribusi
Sistem HARUS memiliki cakupan pengujian yang kuat untuk menjamin bahwa `privacy_vault` disuntikkan dan digunakan secara konsisten dari Engine, ke Swarm, hingga ke Subagent dan Router.

#### Scenario: Subagent di-sandbox mencoba memanggil tool butuh approval
- **WHEN** Subagent dengan zone `SandboxedSubagent` mencoba mengeksekusi tool yang menghasilkan `NeedsApproval`.
- **THEN** Router langsung menolak dan mengembalikan `PermissionDenied` tanpa meminta *approval* pengguna.

### Requirement: Dokumentasi Jujur
Sistem HARUS menyediakan dokumentasi yang secara akurat menjelaskan kondisi fitur yang ada, termasuk arsitektur, kapabilitas, batasan (limitations), dan penanganan privasi/keamanan.

## MODIFIED Requirements
### Requirement: Checkpoint State
State *agent* HARUS menyimpan daftar alat yang telah disetujui (`approved_tools`) pada *checkpoint metadata* agar aman dari proses *restart* (restore-safe).
