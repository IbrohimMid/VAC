# Donor Extraction Map — Stakpak → VAC

> **Status:** skeleton (Step 1 in progress). Tables below have placeholder rows; the next pass fills them in by reading `vendor/stakpak/`.

## Why this document exists

VAC's TUI debt has outpaced the value of incremental polish. We're transplanting Stakpak's shell components — but only the components, not the app — onto VAC's semantic core. This doc is the running ledger of which donor file is reusable as-is, which needs a thin adapter, and which has to be rejected because it leaks Stakpak semantics that conflict with VIL ownership.

See also:

- `crates/vac_shell_contracts/` — adapter trait surface
- `vendor/stakpak/` — read-only donor checkout (Apache 2.0; do not patch in place)

### Pinned donor revision

The vendor directory is gitignored to keep the diff reviewable. Reviewers and contributors clone it once locally:

```bash
git clone https://github.com/stakpak/agent.git vendor/stakpak
git -C vendor/stakpak checkout 2e75bd56970d114ab41653aed045300b9a3257a7
```

When the donor revision is bumped, update both this hash and the changelog entry at the bottom of the document.

## Architectural rule

```
┌──────────────────────────────────────────┐
│  Stakpak donor (read-only, vendored)     │  ← extracted components only
└─────────────┬────────────────────────────┘
              │  trait objects from
              │  vac_shell_contracts
┌─────────────▼────────────────────────────┐
│  vac_shell_bridge (Step 2)               │  ← VAC-owned implementations
└─────────────┬────────────────────────────┘
              │
┌─────────────▼────────────────────────────┐
│  VAC core (vac_core, vac_session_engine, │
│  vac_tools, vil_*)                       │  ← unchanged semantic plane
└──────────────────────────────────────────┘
```

The donor never sees a VAC type and the VAC core never sees a donor type. All traffic is through `vac_shell_contracts` DTOs.

## Reject-on-sight categories (Blocker A/B/C)

These categories are **rejected wholesale**, regardless of file. The donor extraction process will NEVER reuse code that touches them:

| Category | Reason | Owner in target |
|---|---|---|
| `.stakpak/...` path lookups | Blocker A — config duality | `VacPaths` adapter |
| `AutoApproveManager`, donor approval state machine | Blocker B — approval policy duplication | VAC gate + `VacApprovalBridge` |
| `stakai::Model` direct dependency | Model-coupling site | `VacModelView` adapter |
| `commands_to_helper_commands()`, donor's command merge logic | Blocker C — donor's own grammar duality | `VacCommandRegistry` |
| `libs/ai`, `libs/agent-core`, `libs/api`, `libs/gateway`, `libs/server`, `libs/ak` | Stakpak semantic core | VAC keeps ownership |
| `SecretManager` from donor | Privacy/secret model duplication | VAC `privacy_vault` |
| Donor PTY/shell session lifecycle | Conflicts with `vac_shell` | VAC runtime, donor renders only |

## Per-component map

Each row: **donor module/file** · **verdict** · **adapter required (if any)** · **notes**.

Verdicts:
- ✅ **as-is** — copy unchanged into VAC tree
- 🟡 **with adapter** — copy + wire through `vac_shell_contracts`
- ❌ **reject** — do not transplant; rebuild VAC-native

### Phase 1 candidates (highest leverage)

| Donor path | Verdict | Adapter | Notes |
|---|:---:|---|---|
| `tui/src/services/commands.rs` | _TBD_ | `VacCommandRegistry` | Executor pattern is reusable; merge logic is rejected |
| `tui/src/app.rs` | ❌ | — | Whole app state too coupled to donor — extract widgets only |
| `tui/src/services/command_palette.rs` _(or equivalent)_ | _TBD_ | `VacCommandRegistry` | Step 3 proof-of-life target |
| `tui/src/services/sessions*.rs` | _TBD_ | `VacPaths` | Strip `.stakpak/session/...` |
| `tui/src/services/approval*.rs` _(approval list widget)_ | _TBD_ | `VacApprovalBridge` | UI only — no decision logic |
| `tui/src/services/model_switcher*.rs` | _TBD_ | `VacModelView` | Decouple from `stakai::Model` |
| `tui/src/services/plan_mode*.rs` | _TBD_ | `VacPaths` | Plan file lives at `.vac/session/plan.md` |
| `tui/src/services/shell_popup*.rs` | _TBD_ | — | UI only; runtime stays VAC `vac_shell` |

### Phase 2 candidates (deferred)

| Donor path | Verdict | Adapter | Notes |
|---|:---:|---|---|
| `cli/` entry point | _TBD_ | — | May replace `vac_cli` entirely or merge |
| `libs/mcp/*` | _TBD_ | — | Compare against `vac_mcp_core`; pick winner |
| `libs/shell-tool-approvals` | _TBD_ | — | Possibly redundant with VAC gate |
| `tui/src/services/runtime*.rs` | _TBD_ | — | VAC already has runtime tab (#05) |
| `tui/src/services/autopilot*.rs` | _TBD_ | — | VAC owns scheduler state |

### Hard rejects (record-only, no further analysis)

| Donor path | Reason |
|---|---|
| `libs/ai/*` | Stakpak semantic core — VAC owns reasoning |
| `libs/agent-core/*` | Same |
| `libs/api/*` | Stakpak backend coupling |
| `libs/gateway/*`, `libs/server/*` | Hosted-service plumbing |
| `libs/ak/*` | Auth-key store specific to Stakpak |
| Anywhere `~/.stakpak/` is composed | Path duality |
| Anywhere `AutoApproveManager` is constructed | Approval duality |

## Adapter contracts (Step 2)

Defined in `crates/vac_shell_contracts/`:

- `VacPaths` — `.stakpak → .vac` path remap
- `VacModelView` + `ProviderId` — flat projection of VAC LLM config
- `VacApprovalBridge` — pending/resolve API; donor calls, VAC decides
- `VacCommandRegistry` — single source of truth for slash/palette
- `VacShellEvent` + `VacSubmitRequest` — narrow event surface

Implementations land in `vac_shell_bridge` (Step 2, not yet created).

## Step 3 proof target

**Component:** command palette.

Why: highest dogfood frequency, smallest blast radius, exercises both `VacCommandRegistry` (single registry through the donor) and the donor's interaction patterns (filter, fuzzy match, keyboard navigation). If the palette can read from a VAC-owned registry without dragging `AppState` along, the extraction pattern is repeatable for sessions, approvals, model-switcher.

Acceptance:

- Donor palette widget renders entries from a `VacCommandRegistry` impl.
- `Enter` on an entry dispatches a VAC slash action via the bridge.
- Donor's `commands_to_helper_commands()` and `get_all_commands()` are not linked.
- No `.stakpak/` paths reached at runtime.

## Open questions

1. Does the donor palette widget assume the donor `AppState`? If yes, the widget itself is part of the extraction surface (more work) — if no, it's just `VacCommandRegistry` plumbing.
2. Donor's `tui/src/app.rs` `AppStateOptions::model: stakai::Model` — patch upstream or wrap? Decision deferred to Step 2.
3. `cli/` vs `crates/vac_cli/` — eventually one of them survives. Out of scope for Step 1.

## Changelog

- _date TBD_ · skeleton landed.
