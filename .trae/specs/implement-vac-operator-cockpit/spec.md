# Implementasi VAC Operator Cockpit Spec

## Why
VAC perlu ditingkatkan dari TUI yang feature-rich menjadi operator-grade coding cockpit. Tujuannya adalah mencapai parity dengan benchmark di shell/repo/operator UX, lalu unggul di VIL-native observability, governance, validation, dan repair. Fitur baru tidak boleh menambah cabang besar di event loop, dan setiap surface TUI harus punya satu source of truth untuk action, help text, keymap, dan telemetry.

## What Changes
- **Wave 0**: TUI Integrity Sweep untuk menutup debt (audit slash command, deduplikasi key binding, audit InputEvent, audit footer/help).
- **Wave 1**: Unifikasi TUI Kernel agar VAC terasa seperti satu cockpit (ActionRegistry, controller per surface, statusline permanen). **BREAKING**
- **Wave 2**: Repo-Native Coding Workbench untuk coding loop inti (Git Workbench, Patch/Hunk review, Multi-shell manager, Repo navigator).
- **Wave 3**: Repo Intelligence & Context Composer untuk repo cognition (RepoIndexService, Unified search, Context composer, Model capability matrix).
- **Wave 4**: Autonomous Task Graph yang inspectable (TaskGraph eksplisit, Inspector UI, worktree per subtask, Approval policy profiles).
- **Wave 5**: VIL Operator Superpowers untuk keunggulan spesifik di repo VIL (structured issue schema, VIL campaign mode, validation heatmap, rulebook cockpit, semantic repair loop).

## Impact
- Affected specs: TUI command handling, event routing, git workbench, repo intelligence, task execution, VIL integration.
- Affected code: `event_loop.rs`, `app/events.rs`, `view.rs`, `git.rs`, `review.rs`, `search.rs`, `runner.rs`, `engine.rs`, `vil_workbench.rs`, dan modul terkait lainnya.

## ADDED Requirements
### Requirement: Unifikasi TUI dan Kernel
Sistem SHALL menyediakan `ActionRegistry` untuk standardisasi action dari keyboard, slash command, command palette, dan mouse, serta menggunakan controller per surface.

### Requirement: Repo-Native Coding Workbench
Sistem SHALL menyediakan Git Workbench, Patch/Hunk review, Multi-shell manager, dan Repo navigator baseline langsung di dalam TUI.

### Requirement: Repo Intelligence
Sistem SHALL menyediakan `RepoIndexService` untuk path, content, diagnostics, dan symbol index, beserta unified search dan context composer yang eksplisit sebelum pengiriman.

### Requirement: Autonomous Task Graph
Sistem SHALL menyediakan `TaskGraph` eksplisit dengan parent/subtask tree, dependency edges, retry state, dan Inspector UI.

### Requirement: VIL Operator Superpowers
Sistem SHALL menyediakan structured issue schema untuk issue classifier, VIL campaign mode, validation heatmap, dan rulebook cockpit.

## MODIFIED Requirements
### Requirement: TUI Event Handling
**Reason**: Event handling saat ini memiliki duplikasi key bindings dan debt struktural yang menyesatkan.
**Migration**: Menghapus duplikasi key binding, memastikan setiap event memiliki handler yang nyata, dan menghasilkan help/footer hints dari Action Registry.
