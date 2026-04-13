# Rencana Implementasi Donor dari Stakpak ke VAC

## 1. Tujuan
Mengadopsi komponen-komponen krusial dari *codebase* donor `stakpak` ke dalam arsitektur Vastar Agentic CLI (VAC). Tujuan utamanya adalah menutupi kelemahan sistem saat ini, seperti estimasi token yang kurang akurat, serta meningkatkan stabilitas antarmuka (TUI), sandbox eksekusi, dan alur konfirmasi *Human-in-the-loop* (HITL).

## 2. Ruang Lingkup & Area Implementasi
*   **VIL Layer (`crates/vil_swarm`, `crates/vil_llm`):** Perbaikan *context budgeting* dan penanganan *streaming*.
*   **Tools Layer (`crates/vac_tools`):** Pembatasan eksekusi *subprocess* (Sandbox) dan interupsi persetujuan (Approvals).
*   **CLI Layer (`crates/vac_cli`):** Integrasi sistem *Input Handling* & *Keymaps* pada TUI.

## 3. Langkah-Langkah Implementasi

### Fase 1: Upgrade Context Budgeting (Menggunakan Tokenisasi Nyata)
**Masalah:** Saat ini `context_budget.rs` menggunakan rasio statis `BYTES_PER_TOKEN = 3.5` yang tidak akurat.
**Tindakan:**
1. Tambahkan dependensi `tiktoken-rs` (atau tokenizer Rust yang ekuivalen) di `crates/vil_swarm/Cargo.toml`.
2. Ganti logika `message_token_estimate` di `crates/vil_swarm/src/context_budget.rs` agar menggunakan *tokenizer* sungguhan berdasarkan model yang dipilih.

### Fase 2: TUI Input Handling & Keymaps
**Masalah:** Penanganan *keyboard event* `crossterm` di `event_loop.rs` saat ini sangat mentah.
**Tindakan:**
1. Porting sistem abstraksi *Input Parser* dan *Keymap* dari `stakpak`.
2. Terapkan arsitektur *Command Pattern* sehingga *event* dari keyboard diterjemahkan menjadi *Action* TUI yang jelas (menghindari kerumitan *state management* langsung di *event loop*).

### Fase 3: Tool Execution Sandbox
**Masalah:** Belum ada pembatasan waktu (timeout) dan keamanan pada saat agen mengeksekusi shell command.
**Tindakan:**
1. Implementasi sistem *Sandbox* dari `stakpak` ke `crates/vac_tools/src/sandbox.rs`.
2. Pastikan adanya batasan waktu eksekusi (timeout) dan sanitasi *output* standar (stdout/stderr) untuk mencegah agen memblokir *event loop* secara permanen.

### Fase 4: Streaming Abstraction
**Masalah:** Pemrosesan *stream* dari LLM belum optimal dalam memisahkan JSON argumen untuk *tool calls* dan *text delta*.
**Tindakan:**
1. Adopsi mekanisme *chunked stream* dari `stakpak` ke `crates/vil_llm/src/streaming.rs`.
2. Agregasi argument tool secara transparan agar tidak bocor dan merusak *rendering* UI.

### Fase 5: Human-In-The-Loop (HITL) Intercepts
**Masalah:** Eksekusi tugas-tugas berbahaya membutuhkan alur persetujuan (konfirmasi) yang rapi.
**Tindakan:**
1. Integrasikan logika interupsi/persetujuan dari `stakpak` ke `crates/vac_tools/src/approvals.rs`.
2. Hubungkan status *Approval* ini ke antarmuka TUI sehingga agen bisa otomatis melakukan *pause/checkpoint* dan memunculkan modal persetujuan Y/N.

## 4. Pengujian (Testing)
1. **Unit Tests:** Menguji keakuratan estimasi token pada `context_budget.rs` dengan berbagai jenis teks.
2. **Sandbox Tests:** Menguji skenario *timeout* pada command *shell* yang berjalan lebih lama dari batas waktu (misal: `sleep 10s`).
3. **Manual HITL Tests:** Menjalankan agen dengan perintah modifikasi file kritis untuk memicu sistem persetujuan (HITL) pada TUI secara langsung.