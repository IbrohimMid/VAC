# Integrasi Provider Ollama Lokal (Qwen2.5-Coder) untuk VAC CLI/TUI Spec

## Why
VAC saat ini belum memiliki jalur provider LLM lokal yang bisa didogfood end-to-end (CLI dan TUI) seperti Codex CLI / Claude Code. Ollama menyediakan baseline runtime lokal yang umum dipakai untuk uji parity ergonomi dan streaming fidelity.

## What Changes
- Menambahkan provider baru `ollama` sebagai provider LLM lokal (HTTP) yang dapat dipakai oleh `vac exec` dan `vac chat`.
- Menambahkan dukungan konfigurasi minimal untuk Ollama (base URL + model + parameter generasi dasar).
- Menambahkan prosedur uji end-to-end menggunakan `ollama run qwen2.5-coder` untuk memverifikasi streaming, tool calls, dan UX parity dasar.
- Menambahkan guardrail agar test/fitur tetap aman di CI: test provider Ollama HARUS auto-skip bila runtime Ollama tidak tersedia.

## Impact
- Affected specs: CLI command grammar (C1), streaming fidelity (C6), plan/tool gating (C7), approval/hook (C12) untuk memastikan tidak ada regresi saat provider baru dipakai.
- Affected code: `crates/vac_cli`, `crates/vac_session_engine` (layer LLM adapter/provider), `crates/vac_core` (config parse), dan (opsional) `crates/vac_tui_runtime` (statusline/provider label).

## ADDED Requirements
### Requirement: Provider Ollama Lokal
Sistem SHALL menyediakan provider bernama `ollama` yang dapat digunakan oleh VAC untuk mengirim prompt dan menerima stream respons.

#### Scenario: Konfigurasi provider
- **WHEN** operator menambahkan konfigurasi provider Ollama di `.vac/config.toml`
- **THEN** `vac exec --provider ollama ...` menggunakan konfigurasi tersebut tanpa hardcode.

#### Scenario: Streaming respons
- **WHEN** model mengembalikan output secara stream
- **THEN** VAC menampilkan delta teks secara inkremental (tanpa menunggu completion) baik pada CLI maupun TUI.

#### Scenario: Error yang actionable
- **WHEN** runtime Ollama tidak berjalan atau model belum tersedia
- **THEN** VAC menampilkan error yang jelas dan actionable (mis. instruksi `ollama serve` atau `ollama pull <model>`), tanpa panic.

### Requirement: Uji End-to-End dengan Qwen2.5-Coder
Sistem SHALL mendukung pengujian E2E dengan model `qwen2.5-coder` melalui Ollama untuk memvalidasi eksekusi prompt, streaming, dan interaksi tools.

#### Scenario: Jalur `vac exec`
- **WHEN** operator menjalankan `vac exec --provider ollama "..."` pada project contoh
- **THEN** output stream terlihat inkremental dan metadata provider/model terlihat pada statusline/telemetry yang tersedia.

#### Scenario: Jalur `vac chat`
- **WHEN** operator menjalankan `vac chat --provider ollama` (atau mekanisme setara yang sudah ada)
- **THEN** TUI memperlihatkan streaming message dan tool cards muncul segera saat tool diminta.

### Requirement: Kompatibilitas CI
Sistem SHALL menjaga test suite tetap stabil tanpa ketergantungan Ollama di CI.

#### Scenario: Auto-skip
- **WHEN** test provider Ollama dijalankan pada environment tanpa `ollama` binary atau tanpa server yang reachable
- **THEN** test ditandai skipped (bukan failed).

## MODIFIED Requirements
### Requirement: CLI Grammar `vac exec` / `vac chat`
`vac exec` dan `vac chat` HARUS mampu memilih provider lewat flag/config tanpa mengubah semantics sandbox/plan gate yang sudah ada.

## REMOVED Requirements
### Requirement: “Compete sampai setara semua tool”
**Reason**: Tidak terukur dan tidak deterministik dalam satu perubahan.
**Migration**: Diganti dengan metrik verifikasi yang bisa diulang: streaming fidelity, error mapping, dan jalur E2E dengan Ollama/Qwen2.5-Coder.

