# Peta Ekstraksi Donor TUI Codex → VAC (C-TRACK)

> **Status:** Tahap Perencanaan. Dokumen ini memetakan komponen TUI dari Codex untuk diintegrasikan ke dalam ekosistem VAC (C-TRACK).

## Tujuan

Dokumen ini berfungsi sebagai panduan utama untuk mengidentifikasi komponen antarmuka pengguna (TUI) dari arsitektur Codex (atau referensi ekuivalen) yang dapat diadopsi oleh VAC. Tujuan utamanya adalah mencapai paritas fungsionalitas (Codex-Grade VAC TUI Parity) tanpa mengorbankan kepemilikan semantik inti VAC.

## 1. Fitur Codex & Ekuivalen VAC

| Area Fungsional | Fitur Codex | Ekuivalen VAC (Saat Ini / Target) | Status Paritas |
|---|---|---|---|
| **Siklus Agent (Loop)** | `QueryEngine.submitMessage()` (Streaming tools & text) | `vac_session_engine::submit_one` (Single round-trip) → Target: Async Stream | 🔴 Tertinggal |
| **Subagen (Task Tool)** | `AgentTool` (Explore, Plan, Verify) | `vil_swarm` (Internal orchestration) → Target: `AgentTool` publik | 🔴 Tertinggal |
| **Penjadwalan Otonom** | `CronCreate`, `MonitorTool`, `/loop` | Tidak ada ekuivalen → Target: `Cron` & `Monitor` primitive | 🔴 Tertinggal |
| **Sistem Hook** | 9 event hook × 4 tipe perintah | Tidak ada ekuivalen → Target: `HookRegistry` | 🔴 Tertinggal |
| **Ekstensi Kemampuan** | `/skills` (Markdown-based registry) | Bundle internal `vac_tools` → Target: `.vac/skills/*.md` | 🟡 Sebagian |
| **Perangkat Web** | `WebFetchTool`, `WebSearchTool` | Tidak ada ekuivalen | 🔴 Tertinggal |
| **Perangkat Worktree** | `EnterWorktreeTool` | Tidak ada ekuivalen | 🔴 Tertinggal |
| **Bridge Jarak Jauh** | `bridge/` (SSE, JWT, WebSocket) | `vac_bridge` (Hanya ACP) → Target: SSE + `/teleport` | 🟡 Sebagian |
| **UX Time-travel** | `/rewind`, `/thinkback` | `RewindStore` (SQLite) → Target: Ekspor UX | 🟡 Sebagian |

## 2. Pola Ekstraksi Donor (Allowed vs Forbidden)

Mirip dengan ekstraksi Stakpak, komponen TUI dari referensi Codex harus diekstraksi dengan aturan ketat:

### Kategori yang Dilarang (Forbidden / Reject-on-sight)
Pola-pola berikut **TIDAK BOLEH** ditransplantasikan ke VAC dan harus ditolak secara keseluruhan:
1. **State Mesin Spesifik Codex:** Komponen yang secara langsung memutasi status aplikasi global (`AppState`) Codex atau bergantung pada siklus hidup *background task* milik donor.
2. **Coupling Model LLM:** Ketergantungan langsung pada *query engine* atau *provider routing* spesifik Codex (harus menggunakan `vac_session_engine`).
3. **Hardcoded Paths:** Referensi path ke direktori spesifik donor (harus dialihkan menggunakan `VacPaths`).
4. **Manajemen Rahasia (Secrets):** Logika pengambilan token/API key dari donor (harus menggunakan `privacy_vault` VAC).

### Kategori yang Diizinkan (Allowed)
1. **Widget UI Murni (Stateless):** Komponen *render* murni berbasis `ratatui` yang tidak memiliki status selain yang diberikan oleh pemanggil.
2. **Helper Fungsi Murni:** Modul utilitas untuk pemformatan, *parsing*, atau tata letak.
3. **Pola Adaptor (dengan kontrak):** Komponen yang dapat dibungkus dengan *trait* dari `vac_shell_contracts` (misalnya: `VacCommandRegistry`, `VacModelView`).

## Kontrak Arsitektur

```text
┌──────────────────────────────────────────┐
│  Donor TUI Codex (Referensi Komponen)    │  ← Hanya ekstraksi UI/UX
└─────────────┬────────────────────────────┘
              │  Melalui trait
              │  vac_shell_contracts
┌─────────────▼────────────────────────────┐
│  vac_shell_bridge (Adaptor VAC)          │  ← Implementasi dimiliki VAC
└─────────────┬────────────────────────────┘
              │
┌─────────────▼────────────────────────────┐
│  VAC Core (Mesin Semantik, Session)      │  ← Logika agen murni
└──────────────────────────────────────────┘
```
