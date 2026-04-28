# Status Integrasi C-TRACK (Paritas Codex TUI)

> **Pembaruan Terakhir:** 2026-04-28
> **Tujuan:** Melacak status implementasi 12 inisiatif utama (C1-C12) dari cetak biru paritas Codex-Grade VAC TUI (`docs/competitive/CODEX_PARITY_BLUEPRINT.md`).

| ID | Inisiatif (Cetak Biru) | Komponen / *Crate* | Status TUI / Runtime | Catatan Integrasi |
|:---:|---|---|:---:|---|
| **C1** | Streaming Tool-Call Loop | `vac_session_engine`, `vac_tui_runtime` | 🟡 IN PROGRESS | Lapisan *library* (`async_stream`) selesai. Migrasi konsumen aliran TUI sedang berlangsung. |
| **C2** | Kompaksi Konteks Otomatis | `vac_session_engine` | 🟢 IMPLEMENTED | *Circuit breaker* (batas `effectiveContextWindow`) terhubung dengan batas kompaksi. |
| **C3** | Tampilan Tugas di Percakapan | `vac_tui_runtime`, `vac_shell_activity` | 🔴 NOT STARTED | Rencana perutean `TodoTool` ke varian `ActivityItem::Todo` masih tertunda. |
| **C4** | Agen Perangkat *First-Class* | `vac_tools::builtin`, `vil_swarm` | 🟡 IN PROGRESS | Subagen (`Explore`, `Plan`, `Verify`) dapat diinstansiasi di mesin, namun integrasi *sidechain* TUI belum diaktifkan. |
| **C5** | Registri *Skills* (Markdown) | `vac_tools` | 🔴 NOT STARTED | Penguraian `.vac/skills/*.md` (YAML *frontmatter*) dan alat pemuatan (`SkillTool`) belum diintegrasikan. |
| **C6** | Pembatas Mode Rencana | `vac_tools`, UI | 🔴 NOT STARTED | Gerbang `EnterPlanModeTool` dan indikator lencana `plan` belum terhubung ke antarmuka. |
| **C7** | Primitif Cron & Pemantau | `vac_tui_runtime::cron` | 🔴 NOT STARTED | Penjadwalan latar belakang (`.vac/cron.json`) dan *sub-pane* tab Runtime belum dibangun. |
| **C8** | Penjadwalan Sinkronisasi | `vac_tools` | 🔴 NOT STARTED | Perintah `/loop` dan `ScheduleWakeup` untuk agen otonom belum diimplementasikan. |
| **C9** | Sistem Hook Dapat Dikonfigurasi | `vac_session_engine` | 🔴 NOT STARTED | Filter pencocokan `HookRegistry` (9 *event*, 4 tipe perintah) dan perlindungan kotak pasir (sandbox) menunggu desain keamanan. |
| **C10** | Ekstensi Web & Worktree | `vac_tools` | 🟡 IN PROGRESS | `WebFetchTool` memanfaatkan `reqwest` bawaan, `EnterWorktreeTool` masih dalam purwarupa (*prototype*). |
| **C11** | Jembatan Jarak Jauh & UI | `vac_bridge` | 🔴 NOT STARTED | Transpor SSE, rute otentikasi JWT, dan perintah tata letak (`/statusline`, `/output-style`) belum ditambahkan. |
| **C12** | UX Putar Ulang & Inspektur | `RewindStore`, UI | 🔴 NOT STARTED | Integrasi `/rewind`, `/thinkback`, dan tampilan statistik grafik `/context` ke palet antarmuka pengguna belum dikerjakan. |

## Ringkasan Progres
- **IMPLEMENTED:** 1/12
- **IN PROGRESS:** 3/12
- **NOT STARTED:** 8/12

*Catatan: Inisiatif tingkat pustaka (*library layer*) banyak yang telah diselesaikan (berdasarkan jejak `docs/COMPETE_EXECUTION_PLAN.md`), status di atas lebih difokuskan pada penyelesaian ujung-ke-ujung (end-to-end) pada lapisan antarmuka pengguna (TUI).*
