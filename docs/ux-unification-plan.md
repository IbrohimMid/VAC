# UX Unification Plan — making W1–W10 feel like one system

**Written:** 2026-04-24.
**Problem:** the TUI ships with ~45 services, 20+ CLI commands, 6
bundled skills, and 4 autofiring subsystems (AutoDream,
AwaySummary, PassiveFeedback, speculation). Each surfaces through
its own channel — some via `println!`, some via `tracing`, some via
the notifier service, some not at all. The operator sees a
grab-bag of features; there is no single place to answer "what is
the system doing right now?" or "why did that just happen?".

**Thesis:** the primitives are already the right ones. What is
missing is a **backbone** — one aggregation point every subsystem
writes to, one rendering spine every view reads from, and one
front-door (command palette) that exposes every feature
consistently. This turns the feature catalogue into a cockpit.

## Architectural shape

Five layers, each small, each builds on the one below.

### L1 — RuntimeEventBus

One typed enum every subsystem pushes on state change:

```rust
pub enum RuntimeEvent {
    PolicyDeny { rule: &'static str, reason: String },
    RateLimitCooldown { provider: String, until: SystemTime },
    ForkSpeculationStarted { parent: Uuid, reads_hint: Vec<PathBuf> },
    ForkSpeculationFinished { parent: Uuid, reads: usize },
    LspDiagnosticPublished { file: PathBuf, severity: Severity },
    MemoryDreamWritten { path: PathBuf, delta_bytes: u64 },
    AwayResumeSummary { gap_secs: u64, commits: u32 },
    McpElicitationRequested { server: String, kind: String },
    SubagentSpawned { agent: String, depth: u32 },
    ToolDenied { tool: String, reason: String },
    // … one variant per audit-interesting state change
}
```

Bus is `Arc<RwLock<VecDeque<RuntimeEvent>>>` ring-buffered at 256
entries. Cheap to clone (Arc refcount), one source of truth. Each
subsystem that already calls `tracing::warn!` gets an additional
one-liner to `bus.publish(event)`.

### L2 — SystemPulse aggregator

One struct on AppState that derives the **current** state of every
subsystem from the most-recent event of each kind. Computed lazily
from the event ring.

```rust
pub struct SystemPulse {
    pub policy: PolicyPulse,     // ok / warn / blocked + reason
    pub rate_limit: RatePulse,   // per-provider cooldown countdown
    pub fork: ForkPulse,         // idle / speculating(reads N)
    pub lsp: LspPulse,           // languages online + error count
    pub memory: MemoryPulse,     // last-dream age + D=N counter
    pub mcp: McpPulse,           // servers attached + elicitation pending
    pub subagent: SubagentPulse, // depth + breadcrumb count
}
```

Each sub-pulse exposes `render_compact(&self) -> String` (one token
for the statusline) and `render_full(&self) -> Vec<Line>` (for the
overlay).

### L3 — Statusline spine

Rewrite the statusline as a single function reading only from
`SystemPulse`. Target format:

```
[main] policy✓ rate✓ fork● lsp:rs+py mem D=3 mcp:2 sub:0 · cwd
```

Every token is a Pulse's `render_compact`. Width budget: 80 cols.
Colors signal health: green ✓ / yellow ● / red ✗. Click / key
mapping per token opens the right overlay.

Refresh is tied to the event bus revision counter — no wall-clock
polling.

### L4 — System overlay (one drawer)

New overlay key — `Ctrl-P s` or `F9`. Contents:

1. **Top:** current `SystemPulse::render_full()` — one section per
   subsystem, expanded view.
2. **Middle:** last 20 `RuntimeEvent`s as a timeline (ts · subsystem
   · severity · one-line summary).
3. **Bottom:** action rows ("toggle sandbox", "freeze policy",
   "reload plugins", "run fork now", "show memory dir"). Enter
   executes, Tab/Shift-Tab navigates.

Replaces: the scattered ad-hoc overlays currently showing one
subsystem each. Existing specialised overlays (file picker,
command palette) stay — this is for **system state**, not content.

### L5 — Notification router

One function, three lanes:

| Severity | Route |
|---|---|
| `Info` | statusline dot + silent entry in timeline |
| `Warn` | toast (3 s) + statusline flash + timeline entry |
| `Block` | modal overlay that demands acknowledgement + timeline entry |

Every subsystem that today calls `notifier.push` / `tracing::warn` /
`println!` goes through `NotificationRouter::route(event,
severity)`. One vocabulary, one visual hierarchy.

### L6 — Command palette breadth

Every subsystem exposes `palette_entries()` → `Vec<PaletteCommand>`.
Registry builds the full list at startup.

Target: ~60 entries grouped by prefix:

- `policy:` — view, freeze, unfreeze, toggle tool deny
- `rate:` — status, reset backoff <provider>
- `speculation:` — run fork, view cache, clear overlay dir
- `lsp:` — restart <lang>, toggle passive feedback
- `memory:` — list dreams, run consolidator now, toggle autodream
- `mcp:` — list servers, reconnect <name>, view last elicitation
- `bridge:` — auth login <provider>, revoke <provider>
- `skill:` — show <name>, run <name>, list
- `sandbox:` — toggle, view current
- `system:` — show pulse, show timeline, clear events

One keystroke (`Ctrl-P`), one search box, one return — the entire
feature surface reachable through one grammar.

## Milestones

Each milestone is small, independently ships UX improvement, can
be reverted without cascade. Sequence is strict: L1 → L2 → L3 → L4
run serial; L5 and L6 can parallel L3+L4 once L1 lands.

### U1 (L1) — RuntimeEventBus primitive
- `crates/vac_tui_runtime/src/runtime_event.rs` with enum + bus.
- Field on AppState: `event_bus: Arc<RuntimeEventBus>`.
- No wiring yet — just the plumbing. 1 day.

### U2 — Wire existing subsystems to bus
- PolicyTracker.check_deny → `PolicyDeny` event.
- RateLimitTracker.observe_429 → `RateLimitCooldown`.
- ForkSpeculationDriver.speculate → Start + Finish.
- AutoDreamService.tick Wrote → `MemoryDreamWritten`.
- AwaySummaryService.on_resume → `AwayResumeSummary`.
- PassiveFeedbackDriver.tick → `LspDiagnosticPublished`.
- SubagentCoordinator.spawn_child → `SubagentSpawned`.
- TrustGate deny path → `ToolDenied`.
- Each: 5–15 LoC. 2 days across all.

### U3 (L2) — SystemPulse aggregator
- Derives pulse state from the event ring.
- `render_compact` + `render_full` per sub-pulse.
- Unit-tested against synthetic event sequences. 2 days.

### U4 (L3) — Statusline rewrite
- Replace existing statusline content with pulse read.
- Colour palette + width budget.
- Snapshot test on render output. 1 day.

### U5 (L4) — System overlay
- New overlay + keybind.
- Timeline renderer reads event ring.
- Action rows dispatch into existing services.
- 2 days.

### U6 (L5) — NotificationRouter
- One routing function, three lanes.
- Migrate existing `notifier::push` callers. 1 day.

### U7 (L6) — Palette breadth pass
- Add `palette_entries()` method per subsystem.
- Registry builds full list.
- ~60 entries. 2–3 days.

### U8 — Consistency audit
- Every overlay: `Esc` dismisses, `?` shows help.
- Every list: `j/k` or arrows, `/` filter, `Enter` act.
- One PR per crate. 1 day.

### U9 — First-time onboarding
- `vac doctor` suggests: set policy, install LSP servers, enable
  candle feature.
- Inline "apply" keystroke per suggestion. 1 day.

**Total: ~12–14 working days.** Spread across one person that's
2–3 weeks; two people in parallel from U3 onwards, ~1.5 weeks.

## Success metric

A new operator running `vac interactive` for the first time can,
in under 10 minutes and without reading docs:

1. See every live subsystem state from one glance (statusline).
2. Reach every feature from one key (`Ctrl-P`).
3. Read why any auto-action fired (timeline in `Ctrl-P s`).
4. Trust that every overlay behaves the same way (`Esc`, `?`, `/`).

None of this needs new primitives — it's plumbing + rendering on
top of what's already there.

## Non-goals

- **No new LLM features.** This is purely the cockpit.
- **No keybinding rework.** Existing binds stay; consistency pass
  only clarifies behaviour inside overlays.
- **No theme overhaul.** Current palette is fine; we use it
  consistently instead.
- **No backward-incompatible changes.** Every existing overlay +
  command keeps working.

## Risk

The bus-and-pulse approach centralises UX so a regression in one
place breaks the whole cockpit. Mitigation: every renderer is
total — it never panics on missing events, always degrades to a
safe "idle" token. The ring buffer means bus publish is
wait-free for producers. Snapshot tests on statusline + overlay
catch shape regressions.

---

**If you approve this shape, the natural first step is U1 +
U2.POLICY (wire policy deny as the first real producer). That
proves the backbone end-to-end with one subsystem, then the other
seven subsystems follow mechanically.**
