# Tasks

- [x] Task 1: BATCH 1 — PR-3B README honesty + current-state sync
  - Deskripsi: Update `README.md` di branch `feature/pr-3b-readme-honesty-rebuild`.
  - Kebutuhan: Dokumentasikan VAC sebagai VIL-native autonomous coding agent. Berikan summary kapabilitas, known limitations eksplisit, dan security/privacy note. Tidak boleh ada claim palsu/fluff.
- [x] Task 2: BATCH 2 — test persistensi "approved_tools"
  - Deskripsi: Buat test save & load checkpoint di `run_state.rs` pada branch `feature/pr-3c-approved-tools-checkpoint-tests`.
  - Kebutuhan: Isi `approved_tools` dengan nilai, assert setelah restore nilainya tetap ada dan field penting lainnya tidak rusak. Pastikan lulus `cargo test -p vil_swarm`.
- [x] Task 3: BATCH 3 — test shared privacy vault pada jalur subagent
  - Deskripsi: Buat test `build_sandbox_context` dan `build_parent_context` di `subagent.rs` pada branch `feature/pr-3d-subagent-privacy-tests`.
  - Kebutuhan: Bangun PrivacyVault, bangun context via helper, assert context.privacy merujuk vault yang sama. Untuk sandbox, assert env_vars kosong dan zone = SandboxedSubagent.
- [x] Task 4: BATCH 4 — test deny path untuk sandboxed subagent di router
  - Deskripsi: Buat test policy `NeedsApproval` pada `AgentZone::SandboxedSubagent` di `router.rs` pada branch `feature/pr-3e-sandbox-approval-deny-tests`.
  - Kebutuhan: Buat stub policy yang memaksa NeedsApproval. Assert hasil = PermissionDenied.
- [x] Task 5: BATCH 5 — test restore/substitute pada "route" dan "route_approved"
  - Deskripsi: Buat test untuk memastikan router melakukan `restore_value` sebelum execute dan `substitute_value` setelahnya pada branch `feature/pr-3f-router-privacy-roundtrip-tests`.
  - Kebutuhan: Buat tool test yang memantulkan argumen. Uji dua jalur: `route` dan `route_approved`. Assert argumen restored dan hasil substituted.
- [x] Task 6: BATCH 6 — smoke test "VacEngine -> SwarmOrchestrator" shared vault wiring
  - Deskripsi: Buat test pada `engine.rs` untuk memverifikasi injeksi `privacy_vault` pada branch `feature/pr-3g-engine-swarm-privacy-smoke`.
  - Kebutuhan: Test minimal yang membuktikan wiring menggunakan vault dari engine.
- [x] Task 7: BATCH 7 — warning cleanup khusus area yang disentuh PR-3A
  - Deskripsi: Bersihkan warning kompilator pada file PR-3A pada branch `feature/pr-3h-warning-cleanup-targeted`.
  - Kebutuhan: Bersihkan warning di `engine.rs`, `secret_substitution.rs`, `orchestrator.rs`, `redaction.rs`, `run_state.rs`, dan helper subagent. Jangan sweeping besar.
- [x] Task 8: BATCH 8 — docs security/privacy architecture note
  - Deskripsi: Tambahkan file dokumen arsitektur privasi di direktori `docs/` pada branch `feature/pr-3i-privacy-architecture-note`.
  - Kebutuhan: Jelaskan kontrak privasi aktual (source of truth flow, substitution vs restore boundary, checkpoint approval persistence, sandboxed subagent deny, limitations).

# Task Dependencies
- Task 6 depends on Task 1, 2, 3, 4, 5
- Task 7 depends on Task 1, 2, 3, 4, 5
- Task 8 depends on Task 1, 2, 3, 4, 5
