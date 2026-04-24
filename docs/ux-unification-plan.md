# UX Unification Plan — converging the backbone, not inventing one

**Written:** 2026-04-24. **Supersedes the earlier draft at this path
in commit `a445986`.** The earlier draft was greenfield-minded; this
rewrite is grounded in what is already on `main`.

## Status check

Significant backbone already exists. The plan's job is to **converge
it**, not start from zero.

| Layer | Already on main | Gap |
|---|---|---|
| Event spine | `app::events::InputEvent` + `update.rs` routes runtime / MCP / shell / LSP / VIL / task / banner / toast / approvals | No single projection reads from it consistently |
| Root observables | `AppStateRootHandle` in `app::root_handle`, wired via `SubagentCoordinator` (commits `760456b`, `6410f61`) | Only subagents publish; other subsystems unaware |
| Operator surface | `view::operator` renders an Operator panel + Activity timeline; workbench tabs for Approvals / Review / Sessions / Agents / Runtime / Plan / Vil / Vwfd / Signal | Surface exists; grammar across them drifts |
| Command surface | `action_registry::ACTION_SPECS` (canonical), `services::helper_block::vac_commands`, `view::overlays::render_shortcuts` hardcoded, `docs/tui/action_matrix.md` | **Three registries + one doc that drift.** This is the root of fragmentation, not a missing bus |
| Overlay stack | `overlay::OverlayManager` with `MAX_STACK_DEPTH = 2` | Non-negotiable constraint — limits how much the unification can lean on modals |

## Revised thesis

> Backbone exists but is split across `InputEvent` (UI state),
> `AppStateRootHandle` (subagent/root observables), and three
> duplicated action registries. Operators see fragmentation not
> because primitives are missing but because **the projection and
> the command catalogue drift**.

The answer is not a new event plane. The answer is:

1. Converge the registry — single source for ACTION_SPECS.
2. Project existing state into a read-only `SystemPulse`.
3. Route every subsystem's user-visible output through the three
   existing surfaces (activity / toast / banner / root
   observables), not a new modal class.
4. Deep-link statusline tokens to the **tabs and overlays that
   already exist**.

## Architecture: facet-oriented projection

### L0 — Action registry convergence (foundation)

One static truth (`ACTION_SPECS`) + one dynamic extension layer
(custom commands under `.vac/commands/`). Everything else —
`helper_block::vac_commands`, `render_shortcuts`, slash dispatcher,
`docs/tui/action_matrix.md` — is **generated** from the registry
or validated against it.

Acceptance:

- Duplicate `Ctrl+P` binding (today both `OpenFilePicker` and
  "command palette" in some docs) resolved exactly once.
- `render_shortcuts` walks the registry, no hardcoded list.
- Generator emits `docs/tui/action_matrix.md` so doc drift becomes
  a CI error.
- Custom-command discovery stays dynamic but slots into the same
  catalogue at lookup time.

Without this, any "UX unification" regresses the moment someone
adds a command in one place and forgets another.

### L1 — SystemPulse: projection, not storage

`SystemPulse` is a **pure read** over state that already lives in
the runtime:

```rust
pub struct SystemPulse<'a> {
    state: &'a AppState,
    root: &'a AppStateRootHandle,
}

impl<'a> SystemPulse<'a> {
    pub fn facets(&self) -> Vec<SystemFacet> { /* derive */ }
}
```

No ring buffer. No new event enum. The projection pulls from:

- `state.execution.activity` (existing activity timeline)
- `state.core.*`, `state.operator_config.*`
- `state.approvals`, `state.mcp_maps`, `state.vil_domain`,
  `state.workspace.*`
- `root.notifications()`, `root.breadcrumbs()`
- `state.speculation`, `state.team` (W6)
- RateLimitTracker / PolicyTracker snapshots (once wired — W9
  follow-up)

Every subsystem is represented by a **`SystemFacet`**:

```rust
pub enum SystemFacetKind {
    Approvals, RuntimeBackoff, RateLimit, Shell,
    Mcp, Lsp, Vil, Speculation, Subagent,
    Environment, Memory, Policy,
}

pub struct SystemFacet {
    pub kind: SystemFacetKind,
    pub severity: FacetSeverity,   // Ok / Info / Warn / Critical
    pub compact_token: CompactToken, // statusline glyph + color
    pub detail_rows: Vec<Line>,      // expanded view
    pub nav_target: Option<NavTarget>, // where Enter navigates
}
```

`NavTarget` maps to an existing workbench tab or a canonical
overlay — **not** a new one.

### L2 — Statusline reads only from pulse

Replace the existing statusline content with
`SystemPulse::compact_line(budget=80)`. Refresh is tied to the
same AppState revision that already drives UI redraw.

Token deep-links via keyboard shortcut — e.g. `g a` opens the
Approvals tab (already exists), `g m` opens Mcp, `g r` opens
Runtime — rather than each token opening a bespoke drawer.

### L3 — Operator + Activity panel unified under pulse grammar

`view::operator` and the activity panel stay. They are **retargeted**
to read from the same `SystemPulse` + the same severity / wording /
icon taxonomy the statusline uses. No UI shock, no new panel; the
system starts to *feel* coherent because every surface speaks the
same grammar.

### L4 — Routing normalisation (three existing lanes)

One helper, `NotifyRouter::route(event, severity)`, decides between:

| Severity | Lane | Existing mechanism |
|---|---|---|
| Info | `push_activity` | `services::activity`, already wired |
| Warn | `push_activity` + `ShowToast` | `services::toast`, already wired |
| Non-blocking critical | `push_activity` + `ShowBanner` | `app::types::banner`, already wired |
| Operator-decision-required | Existing approval / ask-user modal | `OverlayManager` slot, already wired |

No new modal class. The last lane is reserved for genuine
decisions (approvals, ask-user, reject-reason). Everything else
rides existing surfaces.

### L5 — Navigation via `nav_target()`

Each facet carries a `NavTarget::{Tab(WorkbenchTab), Overlay(OverlayId)}`
pointer. Palette actions, statusline chords, and operator-panel
Enter-behaviour all dispatch through one function.

Deep-link targets already exist:

- Mcp facet → `WorkbenchTab::Signal` or Mcp overlay
- Runtime facet → `WorkbenchTab::Runtime`
- Shell facet → shell popup / session
- Approvals facet → `WorkbenchTab::Approvals`
- Vil facet → `WorkbenchTab::Vil`

### L6 — Contract tests (the real consistency enforcement)

VAC is already contract-test-oriented (`contracts_test`, overlay
contract tests, workbench roundtrip). Extend the pattern:

- One global keychord cannot map to two actions.
- `render_shortcuts` output === registry content.
- Statusline output depends only on `SystemPulse`.
- No module imports `ShowToast` except through `NotifyRouter`.
- Overlay stack depth invariant remains `≤ MAX_STACK_DEPTH`.
- `docs/tui/action_matrix.md` matches registry (generator + diff
  check in CI).

Manual audit replaced by a small file of failing-on-drift tests.

## Milestone sequence

**Strict:** U0 first. Without registry convergence, every other
milestone degrades.

**Parallelisable:** after U0 + U1, U2/U3/U4/U5 can run concurrently.

### U0 — Registry convergence
Scope: `ACTION_SPECS` is the sole static registry;
`vac_commands` + shortcut popup + matrix doc regenerate from it;
custom-command discovery is layered on top. Resolve the
`Ctrl+P` clash.
**Acceptance:** deleting a hardcoded shortcut in `render_shortcuts`
or changing a `helper_block::vac_commands` entry that drifts from
`ACTION_SPECS` causes a CI failure.

### U1 — `SystemPulse` v0, first three producers
Scope: facet model + projection code. Wire facets for
**approvals / runtime-backoff / MCP** — these have the highest UX
pain and data already present in `AppState`. No storage changes.
**Acceptance:** `SystemPulse::facets()` returns a deterministic
slice for a hand-built `AppState`. Test-only.

### U2 — Statusline rewrite on pulse
Scope: statusline reads only from `SystemPulse::compact_line`.
Colour + severity mapped from facet severity. Token-keychord deep
links implemented.
**Acceptance:** snapshot test on compact-line render; no other
input affects it.

### U3 — Operator + Activity panel retargeting
Scope: `view::operator` and activity timeline rewired to consume
the same facets. Wording / icon / severity identical to statusline.
**Acceptance:** contract test asserting every severity glyph used
in Operator/Activity matches the statusline glyph for the same
facet.

### U4 — NotifyRouter
Scope: one routing function. Migrate every direct
`push_activity` + `ShowToast` + `ShowBanner` call site through it.
Subagent + root observables already land here via `AppStateRootHandle`.
**Acceptance:** zero direct `ShowToast` constructions outside
`NotifyRouter` (enforced by a grep-level contract test).

### U5 — `nav_target()` rollout
Scope: every facet provides `nav_target()`. Statusline, palette,
and operator-panel Enter all dispatch through one function.
**Acceptance:** integration test: send `g a` / `g m` / `g r`,
assert correct workbench tab becomes focused.

### U6 — Contract tests
Scope: formalise the invariants above as failing-on-drift tests in
`contracts_test.rs`.
**Acceptance:** CI gates on every invariant from L6.

### U7 — Remaining facets + palette breadth
Scope: facets for the remaining 9 subsystems (LSP, VIL,
speculation, subagent, environment, memory, policy, rate-limit,
shell). Palette entries emitted per facet from the registry
(piggy-backs on U0).

### U8 — Onboarding polish
Scope: `vac doctor` checks + inline "apply" suggestions. `StartupSnapshot`
already carries most inputs. Kept last because it rides on the
unified grammar being stable.

## Non-goals (tightened)

- **No third event plane.** `InputEvent` + `AppStateRootHandle` are
  the spines; anything new is a projection, not storage.
- **No new modal class for non-decision alerts.** `OverlayManager::MAX_STACK_DEPTH = 2`
  is hard. Modal slots stay reserved for approvals / ask-user /
  reject-reason.
- **No new action registry parallel to `ACTION_SPECS`.** Extensions
  layer on top via a dynamic discovery path.
- **No keybinding overhaul.** Existing binds stay; U0 resolves the
  one documented clash (`Ctrl+P`), nothing else moves.
- **No theme / colour overhaul.** We use the existing palette
  consistently — that's the unification, not new chrome.

## Risks (reframed)

| Risk | Severity | Mitigation |
|---|---|---|
| Dual/tri source truth (action registry) | **Highest** today | U0 first; contract tests in U6 |
| Overlay pressure (MAX_STACK_DEPTH=2) | Medium | Non-goal explicitly forbids new modals for alerts |
| Projection lag (if a future contributor tries to cache `SystemPulse`) | Medium | Keep it borrow-only; contract test forbids `Clone`/`Arc` on `SystemPulse` |
| Registry regen drift | Low | Generator runs in CI + diff check against committed `action_matrix.md` |

## First step

**U0 + `SystemPulse` v0 wired for approvals / runtime / MCP.**

Reasons those three:

- Approvals has the most UX-sensitive failure mode (wrong decision
  surfaced badly = lost tool call).
- Runtime backoff is the place operators most often ask "what's
  happening?"
- MCP state is already exposed via multiple surfaces today — it's
  the cleanest demonstration that unification doesn't require
  inventing anything.

Not W9 policy / rate-limit — those primitives aren't wired into
the submit path yet; we'd be projecting state that doesn't exist
end-to-end.

## Success metric

A new operator running `vac interactive` sees, within 10 minutes
and without reading docs:

1. Every live subsystem state from the statusline.
2. Operator panel + Activity timeline using the **same** glyph +
   wording + severity for each subsystem.
3. Every feature reachable through `Ctrl+P` (single registry).
4. Enter on any statusline token lands in the right existing tab
   or overlay.
5. Every overlay behaves the same way (`Esc` dismiss, `?` help,
   `/` filter).

And an engineer touching VAC a year from now finds one registry,
one projection, one router — not three.
