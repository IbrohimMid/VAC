# Tasks
- [x] Task 1: Fase 1 — Auto-Discovery Context Engine
  - [x] SubTask 1.1: Identifikasi titik injeksi context untuk prompt/agent (engine/router) dan definisikan payload context minimal.
  - [x] SubTask 1.2: Implementasi `ContextCrawler` yang mengumpulkan cwd + git summary + perubahan ringkas secara aman dan non-blocking.
  - [x] SubTask 1.3: Wire context ke prompt/agent context dan tambahkan test/unit untuk serializer + fallback non-git.

- [x] Task 2: Fase 1 — Ask-User UX Acceleration
  - [x] SubTask 2.1: Tambahkan filter/fuzzy search untuk daftar opsi panjang (multi-select & single-select).
  - [x] SubTask 2.2: Tambahkan shortcut keyboard untuk operasi umum (select all/none, confirm, cancel, jump to search).
  - [x] SubTask 2.3: Tambahkan test untuk perilaku input + rendering state.

- [x] Task 3: Fase 1 — Runtime Hardening & Guardrails
  - [x] SubTask 3.1: Pastikan error path kritis (MCP/TLS, governance block, reasoning retries) muncul via banner dengan severity tepat.
  - [x] SubTask 3.2: Tambahkan regresi test untuk hardening yang sudah ada (shell buffer bound, secret detection, rate limit).

- [x] Task 4: Fase 2 — Agentic Reasoning Loop (Self-Healing)
  - [x] SubTask 4.1: Definisikan state machine reasoning (attempt/observe/diagnose/plan/retry) dengan batas percobaan.
  - [x] SubTask 4.2: Integrasikan dengan mekanisme approval (HITL) dan logging transcript yang bisa diaudit.
  - [x] SubTask 4.3: Tambahkan test untuk batas percobaan + determinisme transisi state.

- [x] Task 5: Fase 2 — VIL Workstation Advanced
  - [x] SubTask 5.1: Tambahkan score timeline sederhana (ASCII sparkline) dan simpan history di state.
  - [x] SubTask 5.2: Tambahkan view dependency ringkas (rulebook/archetype/deps) yang mudah dipindai.
  - [x] SubTask 5.3: Tambahkan panel log streaming minimal (tail) untuk event penting VIL/engine.

- [x] Task 6: Fase 2 — MCP Preset Registry
  - [x] SubTask 6.1: Definisikan model preset (name, env requirements, transport, args) dan loader dari config.
  - [x] SubTask 6.2: Tambahkan minimal 2–3 preset contoh (mis. GitHub, Jira, CI) sebagai baseline wiring.
  - [x] SubTask 6.3: Tambahkan validasi (missing env) dan observability (banner/warning) saat preset gagal aktif.

- [x] Task 7: Fase 3 — Multi-Agent Workflows
  - [x] SubTask 7.1: Tambahkan scheduler dasar (queue + worker) dengan model peran agent (Dev/QA/Review) dan batas concurrency.
  - [x] SubTask 7.2: Tambahkan surface TUI untuk melihat antrean, status, dan output ringkas per agent.
  - [x] SubTask 7.3: Tambahkan test untuk fairness antrean dan shutdown behavior.

- [x] Task 8: Fase 3 — VIL Governance Enforcement
  - [x] SubTask 8.1: Definisikan policy gate (threshold, aksi yang digate, mode strict/soft) via config.
  - [x] SubTask 8.2: Implementasi enforcement pada jalur aksi berisiko dan tampilkan banner pada saat blok.
  - [x] SubTask 8.3: Tambahkan test untuk policy evaluation dan fallback saat score tidak tersedia.

- [x] Task 9: Fase 3 — Session Collaboration Bundle (Minimal Cloud-Sync)
  - [x] SubTask 9.1: Definisikan format bundle (transcript, approvals, context summary, metadata) dan redaction untuk secret.
  - [x] SubTask 9.2: Implement export/import command dan integrasi TUI untuk feedback.
  - [x] SubTask 9.3: Tambahkan test untuk redaction + round-trip bundle.

- [x] Task 10: Verifikasi & Audit Ulang
  - [x] SubTask 10.1: Jalankan `cargo test` dan perbaiki regresi hingga lulus.
  - [x] SubTask 10.2: Audit ulang surface kapabilitas VAC (TUI/UX/agentic) berbasis perubahan yang masuk dan rangkum gap tersisa vs Stakpak/Claude Code.

# Task Dependencies
- [Task 2] depends on [Task 1] (agar context tersedia untuk UX yang konsisten)
- [Task 4] depends on [Task 1, Task 3]
- [Task 5] depends on [Task 1]
- [Task 7] depends on [Task 4]
- [Task 8] depends on [Task 5]
- [Task 9] depends on [Task 3, Task 7]
- [Task 10] depends on [Task 1..9]
