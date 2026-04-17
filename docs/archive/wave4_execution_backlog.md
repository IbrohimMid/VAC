# Wave 4 Execution Backlog

Berdasarkan review arsitektur dan kesenjangan kontrak produk (engine vs product contract), Wave 4 saat ini memiliki status **BLOCKER** untuk klaim produk.

Dokumen ini memecah temuan menjadi *backlog* eksekusi yang tegas dengan status PASS / PARTIAL / BLOCKER dan urutan patch implementasi.

## Priority 0 — Wajib Sebelum Klaim Wave 4 (BLOCKER)

### 1. Satukan Kontrak Checkpoint
**Masalah:** Saat ini `AgentRunState` memiliki checkpoint kaya yang memulihkan state loop internal secara utuh, namun di product surface (`VacEngine::save_checkpoint`), hanya envelope level session yang disimpan. Mode resume CLI dan TUI tidak melanjutkan loop yang tertunda.
**Aksi:** Buat satu jalur resmi `resume_run_state()` di `vac_core` untuk menyatukan dual semantics.
**File Target:**
- `crates/vac_core/src/engine.rs`
- `crates/vil_swarm/src/run_state.rs`
- `crates/vac_cli/src/commands/resume.rs`
- `crates/vac_cli/src/tui/runner.rs`
**Status Saat Ini:** BLOCKER

### 2. Perbaiki Headless Approval Contract
**Masalah:** `vac runtime jobs` masih konsep. Mode `PatchProposal` dan `AutoFixLowRisk` masih memanggil `run_task()` biasa tanpa kontrak approval/headless eksplisit. Tidak boleh diam-diam auto-reject jika tidak ada channel approval.
**Aksi:** Tambahkan mode approval eksplisit: `interactive`, `deny-on-approval`, `preapproved-scope`.
**File Target:**
- `crates/vac_cli/src/commands/run.rs`
- `crates/vac_cli/src/commands/runtime.rs`
- `crates/vac_runtime/src/executor.rs`
- `crates/vac_core/src/engine.rs`
- `crates/vil_swarm/src/orchestrator.rs`
**Status Saat Ini:** BLOCKER

### 3. Wire `--approve` Flag atau Hapus
**Masalah:** CLI menerima flag `--approve`, tapi handler di `run.rs` menerima parameter tersebut dan sama sekali tidak menggunakannya (jalur tetap `engine.run_task(...)`).
**Aksi:** Wire (hubungkan) flag `--approve` agar berfungsi sesuai janjinya, atau hapus flag tersebut agar tidak menyesatkan.
**File Target:**
- `crates/vac_cli/src/commands/run.rs`
**Status Saat Ini:** BLOCKER

### 4. Ikat Knowledge ke Repo Target, Bukan Current Process Dir
**Masalah:** `KnowledgeTool` membangun knowledge base dari `std::env::current_dir()`, bukan dari `project_root` pada `ToolContext`. Ini salah untuk `vac -C /path/to/project`.
**Aksi:** KnowledgeTool harus memuat melalui `ToolContext.working_dir` atau `project_root` yang diinjeksikan oleh engine.
**File Target:**
- `crates/vac_tools/src/builtin/knowledge.rs`
**Status Saat Ini:** BLOCKER


## Priority 1 — Menaikkan Status 4D / 4A (PARTIAL -> PASS)

### 5. Naikkan Validator Menjadi Semantic Guard (VIL-Native)
**Masalah:** Validator lebih mirip "style guard" daripada semantic conformance engine. Belum memeriksa generated plumbing, semantic macro coverage, dan tri-lane consistency secara utuh.
**Aksi:** Tambahkan passes untuk VIL-native conformance.
**File Target:**
- `crates/vil_validate/src/passes.rs`
- `crates/vil_swarm/src/semantic.rs`
- `crates/vac_tools/src/builtin/vil_lsp_query.rs`
**Status Saat Ini:** PARTIAL

### 6. Jadikan Diff Preview Benar-benar Operasional
**Masalah:** Diff UX di TUI masih minim dan hanya line-by-line renderer. `preview_file_diff` didefinisikan tapi belum dipakai aktif.
**Aksi:** Integrasikan `file_diff.rs` ke approval dialog file write/edit atau restore flow.
**File Target:**
- `crates/vac_cli/src/tui/runner.rs`
- `crates/vac_cli/src/tui/services/file_diff.rs`
**Status Saat Ini:** PARTIAL

### 7. Rapikan Canonical Operator-Facing Terms
**Masalah:** Deskripsi `vil_knowledge` tool masih menyebut `VX_APP` dan `SDK_PIPELINE` yang merupakan istilah legacy, bukan terminologi canonical (seperti `VilServer`, `vil-expr`).
**Aksi:** Ubah teks operator-facing tool dan prompt agar canonical penuh (internal ID boleh tetap sama).
**File Target:**
- `crates/vac_tools/src/builtin/knowledge.rs`
- `crates/vil_swarm/src/orchestrator.rs`
**Status Saat Ini:** PARTIAL


## Priority 2 — Packaging & Documentation

### 8. Sinkronkan README dan Onboarding dengan CLI Nyata
**Masalah:** Docs mendorong `vac autopilot` secara langsung, padahal aktualnya membutuhkan subcommand `up|down|status`. Klaim resume semantics dan batasan headless approval juga belum akurat.
**Aksi:** Perbaiki dokumentasi agar 100% jujur dengan kapabilitas CLI.
**File Target:**
- `README.md`
- `docs/onboarding.md`
**Status Saat Ini:** BLOCKER (karena kejujuran produk adalah syarat 4A)
