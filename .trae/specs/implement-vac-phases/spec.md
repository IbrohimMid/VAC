# VAC Implementation Plan Spec

## Why
Untuk menyelesaikan semua sisa pengembangan (fase 1 hingga fase 4) VAC (Vastar Agentic CLI) sesuai dengan `IMPLEMENTATION_PLAN.md` hingga mencapai target 100% complete, memberikan fungsionalitas CLI setara produk komersial dengan kapabilitas native VIL, dan memberikan UX/UI yang stabil dan responsif bagi pengguna.

## What Changes
- Implementasi fitur-fitur **Phase 1 (Product Shell Parity)**: Integrasi Clipboard Paste, Text Selection, Side Panel interactif, dan Message Actions.
- Implementasi fitur-fitur **Phase 2 (Agent CLI Parity)**: Run Async Polish, Isolation UI presets, MCP Admin Surface di TUI, dan Polish pada Shell/Operator Loop.
- Implementasi fitur-fitur **Phase 3 (VIL-Native Superpowers)**: VIL Project Awareness, VIL Review Workstation (Validation display, IR violations), VIL-Aware Code Actions, dan VIL-Native Planning Policy.
- Implementasi fitur-fitur **Phase 4 (Claude-Code-Class Polish)**: State Recovery (safer pending approval, checkpoint resume), Performance (Rendering cache, file search), dan Default/Ergonomic Onboarding.

## Impact
- Affected specs: Fitur shell TUI, kapabilitas command line (`run`, `isolation`, `mcp`), interaksi VIL (`vil_validate`, IR codegen), dan state recovery (`runner.rs`, `checkpoint.rs`).
- Affected code: `crates/vac_cli/src/tui/*`, `crates/vac_cli/src/commands/*`, `crates/vil_validate/*`, `crates/vac_core/*`, `crates/vil_swarm/*`.

## ADDED Requirements
### Requirement: Helper & Input Ergonomics
Sistem SHALL menyediakan kemampuan paste dari clipboard, seleksi teks via mouse, dan command history yang tersimpan.

### Requirement: VIL-Native Workstation
Sistem SHALL memvalidasi IR semantics dan menampilkan findings/violations di dalam TUI, serta menyediakan automated "Code Actions" (seperti *Repair VIL Contract*).

### Requirement: Recovery & Reliability
Sistem SHALL dapat me-resume state (termasuk shell dan pending approvals) setelah interupsi atau crash secara aman.

## MODIFIED Requirements
### Requirement: Side Panel UX
Side panel akan dimodifikasi agar dapat diklik (collapse/expand headers) dan menampilkan lebih banyak status (Sessions, MCP Badges, VIL Validation).
