# PRD — TUI Interface

**Feature Area:** TUI Layer (`crates/vac_tui_runtime`)  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

The VAC TUI is a full-screen terminal interface built on [ratatui](https://ratatui.rs/). It provides streaming conversation, workbench panels, overlay modals, and rich input editing — all surfacing real-time agent state with no placeholder values.

---

## Layout

```
┌──────────────────────────────────────────────────────────┐
│  Conversation pane (streaming markdown, tok/s meter)     │
├─────────────────────────────┬────────────────────────────┤
│  Side Panel                 │  Workbench Tabs            │
│  • Session info             │  Review / Approvals /      │
│  • Activity log             │  Sessions / Agents /       │
│  • Task tray                │  Runtime / VIL / VWFD      │
│  • Sections (collapsible)   │                            │
└─────────────────────────────┴────────────────────────────┘
│  Input editor                            [chips] [status] │
└──────────────────────────────────────────────────────────┘
```

The side panel can be collapsed. Sections within the panel toggle individually on click.

---

## Workbench Tabs

| Tab | Contents |
|-----|----------|
| **Review** | Syntax-highlighted diff, image preview (Kitty), VWFD semantic diff |
| **Approvals** | Tool call queue: args, result preview, approve / reject / approve-all |
| **Sessions** | Resume from checkpoint, fuzzy search, date filter, snapshot metadata |
| **Agents** | Active swarm agents, lane/role labels, per-agent status |
| **Runtime** | Background job queue, job detail, cancel / retry actions |
| **VIL** | Diagnostics viewer, severity levels, repair proposals |
| **VWFD** | Workflow tree (40/60 split), execution mode badge, jump-to-source |

---

## Input System

The input editor supports:

| Prefix | Behavior |
|--------|----------|
| `@<file>` | Insert file context chip |
| `/<command>` | Invoke a `/`-command (slash command palette) |
| `!<shell>` | Run a shell command inline |
| _(plain text)_ | Chat message to the agent |

The four-stage event router processes input in priority order:

1. **Overlay** — captures all keys while any overlay is visible
2. **Workspace** — workspace-level bindings (tab switch, panel collapse)
3. **Workbench** — per-tab bindings (scroll, approve, jump)
4. **Global** — universal fallbacks (quit, help, command palette)

---

## Overlays

20+ modal overlays available, triggered by keybinding or `/<command>`:

| Overlay | Trigger |
|---------|---------|
| Command palette | `Ctrl+P` |
| File picker | `@` prefix or `Ctrl+O` |
| Model switcher | `Alt+M` |
| Profile selector | `Alt+P` |
| Rulebook picker | — |
| Isolation config | — |
| Task tray | `Ctrl+T` |
| Theme picker | — |
| Keybinding help | `Alt+H` / `?` |
| Session search | `Ctrl+R` |
| Shortcut cheatsheet | `F1` |

Overlays stack; pressing Escape dismisses the topmost.

---

## Streaming

- Token streaming from LLM providers renders incrementally in the conversation pane
- A `tok/s` meter shows current throughput
- All streaming paths are async-safe; no blocking I/O on the event thread
- Partial content is always visible — no "loading..." placeholders

---

## Kitty Graphics

At startup (before `EnterAlternateScreen`), `probe_terminal_kitty_support` sends a DCS capability query with a 200ms deadline:

- If supported: images are emitted via chunked base64 DCS sequences after each `terminal.draw()` call, at the cell coordinate returned by the Review tab layout
- If unsupported: `render_ascii_fallback` returns a framed placeholder block

An LRU cache (`KittyImageCache`, capacity-bounded) deduplicates identical image+position pairs across frames to avoid re-encoding base64 on every render.

---

## Session Recorder

Every TUI session can be recorded to a `.jsonl` file capturing all inputs and outputs. Recorded sessions can be replayed in read-only mode:

```bash
vac interactive --replay .vac/sessions/2026-04-22T10:00:00.jsonl
```

---

## Mouse Support

Click dispatch routes to the hit region:

| Region | Action |
|--------|--------|
| Workbench tab bar | Switch active tab |
| Approval row | Select approval for action |
| Session row | Select session for resume |
| Side panel section header | Toggle section collapsed/expanded |
| Task tray overlay row | Scroll or select job |
| Review diff | Jump to file |

---

## Theme & Keybindings

Both theme and keybindings are hot-reloaded from TOML files on save:

- **Theme:** `.vac/theme.toml` — multi-preset (light / dark / high-contrast)
- **Keybindings:** `.vac/keybindings.toml` — user-defined overrides

All color values are sourced from the theme registry. No raw `Color::` literals appear in renderer code.

---

## Boot Truthfulness Contract

The TUI enforces a hydration gate: every pane renders skeleton state (loading indicators) until real data arrives. Once data is present, skeletons are replaced atomically. UI never renders stale or placeholder values as live data.

---

## Terminal Capabilities Registry

`TerminalCapabilities` is populated once at startup and stored on `StartupSnapshot`:

| Capability | Detection Method |
|-----------|-----------------|
| `kitty_graphics` | DCS probe (200ms deadline) |
| `true_color` | `$COLORTERM` env var |
| `unicode_width` | Terminal introspection |

Renderers query `StartupSnapshot::kitty_graphics` and `TerminalCapabilities` to select the appropriate rendering path.
