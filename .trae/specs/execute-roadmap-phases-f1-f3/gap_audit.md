# Gap Audit VAC (TUI/UX/Agentic) vs Stakpak & Claude Code

Tanggal: 2026-04-17  
Ruang lingkup: audit cepat berbasis repo ini + rujukan publik resmi untuk Stakpak/Claude Code.

## Snapshot kapabilitas VAC (berdasarkan repo)

### TUI/UX operator (vac_cli)
- TUI full-screen dengan command palette, tabs, panel, diff/changelog UI, plan/review overlay, dan flow approvals di layer UI ([tui](file:///workspace/crates/vac_cli/src/tui)).
- Slash commands operator yang relevan: `/runtime`, `/shell`, `/plan`, `/plan-review`, `/changes`, `/file-changes`, `/files`, `/review` (lihat [helper_block.rs](file:///workspace/crates/vac_cli/src/tui/services/helper_block.rs#L48-L194)).
- Shell PTY interaktif di TUI + prompt/password prompt detection (lihat [shell_mode.rs](file:///workspace/crates/vac_cli/src/tui/services/shell_mode.rs)).
- Integrasi runtime tab untuk jobs/autopilot (lihat [runtime_operating_guide.md](file:///workspace/docs/runtime_operating_guide.md) dan surface TUI di [view.rs](file:///workspace/crates/vac_cli/src/tui/view.rs)).

### Agentic control-plane
- Orkestrator multi-agent “tri-lane” (Trigger/Data/Control), approval events, checkpoint/resume, iteration cap, dan context budgeting (lihat [vil_swarm](file:///workspace/crates/vil_swarm/src) khususnya [orchestrator.rs](file:///workspace/crates/vil_swarm/src/orchestrator.rs) dan [run_state.rs](file:///workspace/crates/vil_swarm/src/run_state.rs)).
- Workspace context crawler minimal (CWD + snapshot git status/branch/changes) untuk prompt injection (lihat [context_crawler.rs](file:///workspace/crates/vil_swarm/src/context_crawler.rs)).
- Runtime jobs (cron/filewatch/oneshot) + autopilot daemon state file/queue persistence (lihat [vac_runtime](file:///workspace/crates/vac_runtime) dan [runtime.rs](file:///workspace/crates/vac_cli/src/commands/runtime.rs)).

### Guardrails & trust
- Secret detection + substitution berbasis regex (AWS key, api key generic, IP, email) dan mekanisme restore/substitute boundary (lihat [secret_detector.rs](file:///workspace/crates/vac_core/src/security/secret_detector.rs), [secret_substitution.rs](file:///workspace/crates/vac_core/src/security/secret_substitution.rs), dan [privacy_architecture.md](file:///workspace/docs/privacy_architecture.md)).
- Mode isolation (host vs isolated) + policy jaringan (mis. restricted-offline) + trust classes untuk MCP (lihat [docs/milestone3](file:///workspace/docs/milestone3)).

## Gap tersisa vs Stakpak (jujur, berbasis perbandingan fitur publik)

Stakpak diposisikan sebagai “AI DevOps agent” dengan autopilot 24/7, network sandbox/proxy policy (“Warden”), secret substitution “210+ secret types”, dan full session audit logs sebagai pilar utama ([stakpak.dev](https://stakpak.dev/?utm_source=showmebest.ai&utm_medium=referral), [Stakpak vs Coding Agents](https://stakpak.gitbook.io/docs/archive/stakpak-vs-coding-agents)).

### Gap utama
- Network sandbox ala proxy + policy language: VAC punya mode “restricted-offline” dan boundary isolation, tapi repo ini tidak menunjukkan proxy transparan yang memediasi *setiap* network call dengan policy engine setara “Warden”.
- Cakupan secret substitution: VAC saat ini tampak fokus ke subset pola umum (AWS key, api key generic, IP, email). Stakpak mengklaim cakupan secret types jauh lebih luas dan hardening untuk produksi.
- Ops-native primitives: Stakpak menonjol di incident response, health checks, cost watchdog, dsb; VAC saat ini lebih “coding/control-plane” dan belum terlihat punya domain model DevOps/IaC-first yang kaya secara built-in (walau bisa di-extend via tools/MCP).
- “Autopilot” sebagai produk 24/7 yang benar-benar operasional: VAC punya daemon + scheduler + queue, tetapi belum terlihat integrasi channel notifikasi (mis. Slack), runbook ops tingkat produk, dan loop auto-healing yang matang seperti positioning Stakpak.

### Area di mana VAC sudah dekat/mengejar
- TUI operator + approvals: VAC sudah punya shell PTY interaktif + approvals + runtime tab, dan basis TUI jelas mengambil banyak desain/komponen dari donor Stakpak (lihat [LICENSE_ATTRIBUTION.md](file:///workspace/crates/vac_cli/src/tui/LICENSE_ATTRIBUTION.md)).
- Rulebooks: VAC punya konsep rulebook (lihat [rulebook.rs](file:///workspace/crates/vac_cli/src/commands/rulebook.rs)), walau format/fitur dan tingkat “ops intelligence” dibanding Stakpak belum bisa disimpulkan hanya dari repo ini.

## Gap tersisa vs Claude Code (jujur, berbasis rujukan publik Anthropic + repo)

Claude Code secara publik menekankan multi-surface (terminal + VS Code + JetBrains + desktop/web), diff review yang mulus, checkpoints/rewind, serta subagents/hooks/background tasks di produk ([Claude Code product page](https://claude.com/product/claude-code), [Enabling Claude Code to work more autonomously (2025)](https://www.anthropic.com/news/enabling-claude-code-to-work-more-autonomously)).

### Gap utama
- Multi-surface/IDE: VAC punya `vac acp` untuk server editor-facing (lihat [main.rs](file:///workspace/crates/vac_cli/src/main.rs)), tetapi repo ini belum menunjukkan plugin/extension resmi setara VS Code/JetBrains yang “batteries-included”.
- UX checkpoints/rewind: VAC punya checkpoint/resume dan “restore file” CLI, tapi belum terlihat UX “rewind cepat” (mis. shortcut/command untuk memilih restore code vs conversation) yang dipromosikan Claude Code.
- Auto-discovery & workflow default: Claude Code menekankan eksplorasi codebase + penggunaan build/test tools sebagai alur natural. VAC punya context crawler snapshot git dan tools, tetapi banyak workflow (lint/test/triage) masih terlihat bergantung pada prompt/aturan yang eksplisit.
- Subagents/hooks sebagai productized surface: secara internal VAC punya orkestrasi multi-agent dan job runtime, tetapi “hooks” (trigger otomatis per event dalam pipeline user flow) dan “background tasks” yang terasa sebagai fitur produk end-user belum terlihat setara matang.

### Area di mana VAC sudah setara/searah
- Approval gates: VAC punya approval flow untuk tool execution dan trust posture (konsep serupa “you’re in control / explicit approval” yang dipromosikan Claude Code).
- Session persistence & resumability: VAC punya state + checkpoint file + runtime queue persistence (konsep dasarnya searah, meski UX/fitur detail berbeda).

## Gap spesifik internal VAC (yang terlihat langsung dari repo)
- Ada dua jalur komponen TUI: `services/` aktif dan `services_stakpak_disabled/` sebagai donor cadangan. Ini memberi fleksibilitas, tapi juga menambah beban maintenance dan risiko divergence bila belum ada strategi deprecate yang jelas ([tui](file:///workspace/crates/vac_cli/src/tui)).
- Memory/RAG terlihat ada di level crate (`vil_memory`, `vil_rag`) dan artefak `.vac/memory.semantic.db`, namun surface user-facing untuk “memory blocks” ala produk belum tampak jelas di TUI command surface saat ini.
- CLI parity masih menyisakan backlog yang tercatat di dokumen rencana (mis. polish tertentu) sehingga kualitas “produk harian” masih sangat bergantung pada kelengkapan finishing UX (lihat [IMPLEMENTATION_PLAN.md](file:///workspace/IMPLEMENTATION_PLAN.md)).

## Rekomendasi prioritas (untuk mengecilkan gap)
- Claude Code parity (P1): UX “rewind/restore” yang jelas di TUI + command, dan bridge editor (ACP → minimal extension / dokumentasi integrasi).
- Stakpak parity (P1): hardening trust untuk networked tools (minimal allowlist domain/endpoint + audit trail yang mudah diinspeksi), dan perluasan secret detection coverage.
- Product polish (P1): rapikan/konvergensikan jalur TUI (kurangi duplikasi `services_stakpak_disabled/` jika tidak lagi dipakai), serta jadikan workflow lint/test/review lebih “default-on” via hook atau preset profile.

