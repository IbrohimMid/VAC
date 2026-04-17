# Execute Roadmap Phases 1–3 Spec

## Why
VAC sudah memiliki fondasi TUI/UX dan operator-surface yang kuat, namun masih memiliki gap terhadap Stakpak dan Claude Code pada konteks otomatis, otonomi agentic (self-healing), orkestrasi multi-agent, dan kolaborasi. Inisiatif ini mengeksekusi fase 1–3 pada roadmap untuk meningkatkan kapabilitas inti secara bertahap dan terverifikasi.

## What Changes
- Fase 1: Auto-discovery context engine untuk meningkatkan context awareness tanpa instruksi manual.
- Fase 1: Peningkatan Ask-User UX untuk menurunkan friksi HITL (fuzzy search, shortcuts, rendering/paging).
- Fase 1: Hardening runtime lanjutan (observability error paths, guardrails default, dan regresi test).
- Fase 2: Agentic reasoning loop (self-healing) dengan batas percobaan dan integrasi approval.
- Fase 2: Workstation VIL tingkat lanjut (dependency view, score timeline, log streaming).
- Fase 2: Registry “MCP preset” bawaan (mekanisme preset + beberapa preset minimal) untuk mempercepat integrasi tool eksternal.
- Fase 3: Multi-agent workflows (scheduler + queue visibility di TUI) untuk menjalankan peran agent (Dev/QA/Review) secara terstruktur.
- Fase 3: VIL governance enforcement sebagai gate untuk aksi berisiko (apply/deploy/merge-path) berbasis threshold score dan policy.
- Fase 3: Cloud-sync & team collaboration minimal (export/import session bundle) dengan fondasi untuk sinkronisasi real-time di iterasi berikutnya.

## Impact
- Affected specs: TUI Operator UX, Context Awareness, LLM Orchestration, MCP Extensibility, VIL Governance
- Affected code:
  - `crates/vac_core` (context crawler, multi-agent scheduler, governance checks)
  - `crates/vil_llm` (reasoning loop, throttling/guardrails integration)
  - `crates/vac_cli` (Ask-User UX, TUI surfaces untuk agent queue + VIL workstation advanced, session export/import UX)
  - `crates/vac_tools` / MCP wiring (preset registry + wiring)

## ADDED Requirements
### Requirement: Auto-Discovery Context Engine
Sistem SHALL mengumpulkan konteks proyek secara otomatis (minimal: cwd, git branch/status, diff summary, file changeset ringkas) dan menginjeksikannya ke lapisan prompt/agent context.

#### Scenario: VAC berjalan pada repo git
- **WHEN** VAC dimulai di working directory yang merupakan repo git
- **THEN** agent context mencakup ringkasan branch, status dirty/clean, serta ringkasan perubahan.

### Requirement: Ask-User UX Acceleration
Sistem SHALL mendukung pemilihan opsi cepat untuk ask-user (single/multi select) melalui shortcut keyboard dan pencarian (fuzzy/filter) untuk daftar opsi panjang.

### Requirement: Agentic Reasoning Loop (Self-Healing)
Sistem SHALL menyediakan loop reasoning terstruktur yang dapat melakukan percobaan eksekusi, mendeteksi error, melakukan diagnosis, dan mencoba ulang dalam batas maksimal sebelum meminta intervensi operator.

### Requirement: MCP Preset Registry
Sistem SHALL menyediakan mekanisme preset konfigurasi MCP yang dapat diaktifkan melalui konfigurasi/CLI, sehingga integrasi tool eksternal tidak perlu konfigurasi manual penuh.

### Requirement: Multi-Agent Workflow Scheduler
Sistem SHALL menyediakan scheduler yang mampu menjalankan beberapa peran agent (mis. Dev/QA/Review) dengan antrean tugas yang terlihat di TUI.

### Requirement: VIL Governance Enforcement
Sistem SHALL memblokir aksi yang didefinisikan sebagai “berisiko” ketika VIL score berada di bawah threshold atau policy melarang, dan SHALL menampilkan banner yang menjelaskan alasan blok.

### Requirement: Session Collaboration Bundle (Minimal Cloud-Sync)
Sistem SHALL mendukung export/import bundle sesi (transcript + approvals + context ringkas) agar sesi bisa dipindahkan antar mesin/operator tanpa backend.

## MODIFIED Requirements
### Requirement: Operator Observability
Sistem SHALL memunculkan kondisi error yang relevan (mis. MCP TLS failure, governance block, reasoning retries) secara eksplisit pada operator surface (banner/workbench) dengan severity yang sesuai.

## REMOVED Requirements
Tidak ada requirement yang dihapus pada perubahan ini.

