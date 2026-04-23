# Eksekusi VAC Ultraplan Superbatch Spec

## Why
Diperlukan eksekusi secara otonom untuk keseluruhan rencana (plan) yang terdapat di dalam `docs/agent-superbatch-prompt.md`. Sistem harus menyelesaikan seluruh milestone yang dijabarkan dalam dokumen tanpa berhenti hingga tuntas, mencakup Wave 1, Wave 2, dan Wave 3.

## What Changes
- Mengimplementasikan seluruh milestone (M1-M14, P1-P3) dari `docs/ultraplan-vac-product.md` secara berurutan.
- Membuat dan memelihara `.vac/agent-journal.md` untuk mencatat status setiap milestone.
- Memperbarui `docs/adoption-score.md` secara berkala seiring berjalannya progres.
- Menjalankan `cargo check` dan `cargo nextest` pada setiap langkah.

## Impact
- Affected specs: Fungsionalitas inti, integrasi MCP, manajemen memori, dan fitur Autopilot.
- Affected code: Berbagai crate di dalam workspace termasuk `vac_cli`, `vac_core`, `vac_mcp_core`, `vil_memory`, dll.

## ADDED Requirements
### Requirement: Eksekusi Otonom Milestone
Sistem SHALL menjalankan implementasi kode sesuai urutan di dokumen dan mencatat progres di jurnal.

#### Scenario: Success case
- **WHEN** agent memproses satu milestone
- **THEN** kode diubah, tes dijalankan dan lulus, jurnal diperbarui, dan perubahan di-commit.
