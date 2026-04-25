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
| **Manager / worker / controller state machines** | Hidden coupling to donor `AppState`, background tasks, and lifecycle hooks. **Reject by default unless proven stateless.** Keep widgets, drop their orchestrators. | Re-implemented VAC-side |

## Per-component map

Each row: **donor module/file** · **verdict** · **extraction shape** · **adapter required (if any)** · **provenance** · **notes**.

Verdicts:
- ✅ **as-is** — copy unchanged into VAC tree
- 🟡 **with adapter** — copy + wire through `vac_shell_contracts`
- ❌ **reject** — do not transplant; rebuild VAC-native

Extraction shapes:
- **widget only** — pure render, no state outside what its caller passes
- **pure helper** — stateless function / utility module
- **service + worker** — request/response service that owns a background task
- **stateful controller** — module that mutates donor `AppState` or holds long-lived state
- **whole-app coupling** — only meaningful inside the donor app, can't be lifted

Provenance: short attribution note recorded *when the file is actually copied* (donor commit + path + license tag). Empty until the row's verdict moves off `_TBD_`.

### Phase 1 candidates (highest leverage)

| Donor path | Verdict | Shape | Adapter | Provenance | Notes |
|---|:---:|---|---|---|---|
| `tui/src/services/commands.rs` | 🟡 | pure helper | `VacCommandRegistry` + `VacPaths` | — | Pure dispatcher (`execute_command`, `filter_commands`, `CommandAction` enum). Rejects: `commands_to_helper_commands()` legacy merge, hardcoded `.stakpak/session/plan.md`, indirect `AutoApproveManager` reach. AppState param must be replaced by a narrow context type at extraction. |
| `tui/src/app.rs` | ❌ | whole-app coupling | — | — | Monolithic AppState — pulls every service, `stakai::Model`, `AutoApproveManager`, `SecretManager`. Reject as a unit; lift inner widget/state substructs individually. |
| `tui/src/services/helper_dropdown.rs` _(palette UI lives here, not under a "command_palette" name)_ | ✅ | widget only | none (consumes `VacCommandRegistry` output via caller) | — | **Step 3 proof target.** Pure ratatui dropdown; reads filtered helpers from caller-supplied state. Zero business logic. Extracts cleanly if caller passes a `Vec<ShellCommandSpec>` instead of donor `AppState.input_state`. |
| `tui/src/services/shortcuts_popup.rs` _(sessions list lives here, no dedicated module)_ | 🟡 | widget only | `VacCommandRegistry` (commands section), `VacPaths` (sessions section reads session dir) | — | Unified drawer with three sections (Commands / Shortcuts / Sessions). Sessions section is a read-only list widget — no manager. Extract per-section render fns; keep `Shortcut` struct. |
| `tui/src/services/approval_bar.rs` | 🟡 | widget only | `VacApprovalBridge` | — | Pure tab/toggle widget over `ToolCall` identities. `ApprovalStatus` + `ApprovalAction` enums reusable. Inject queue snapshot; donor never decides. |
| `tui/src/services/auto_approve.rs` _(decision logic — flagged per audit)_ | ❌ | stateful controller | — | — | `AutoApproveManager` reads/writes `.stakpak/.auto-approve.json`, holds mpsc + config_path, makes decisions. Hard reject by the manager rule. Only the `AutoApprovePolicy` enum may travel if proven stateless during extraction. |
| `tui/src/services/model_switcher.rs` | 🟡 | widget only | `VacModelView` | — | Pure popup over a model list. Hard-imports `stakai::Model` for metadata; adapter must project `VacModelView` into a stand-in struct or fork the file at copy time to drop the donor type alias. |
| `tui/src/services/plan.rs` | 🟡 | pure helper | `VacPaths` | — | YAML front-matter parser + `plan_file_exists` / `read_plan_file`. Hardcoded `.stakpak/session/plan.md` becomes an injected `VacPaths::plan_file()`. Extract `PlanMetadata`, `PlanStatus`, `parse_plan_front_matter`. |
| `tui/src/services/shell_popup.rs` | ✅ | widget only | none | — | Stateless height/layout + render. Reads `shell_popup_state` + `shell_runtime_state` as params; no manager. `shell_mode.rs` is the *runtime/PTY* concern (already on the reject list — VAC `vac_shell` owns it). |

### Phase 2 candidates (deferred)

| Donor path | Verdict | Shape | Adapter | Provenance | Notes |
|---|:---:|---|---|---|---|
| `cli/` entry point | _TBD_ | _TBD_ | — | — | May replace `vac_cli` entirely or merge |
| `libs/mcp/*` | _TBD_ | _TBD_ | — | — | Compare against `vac_mcp_core`; pick winner |
| `libs/shell-tool-approvals` | _TBD_ | _TBD_ | — | — | Possibly redundant with VAC gate |
| `tui/src/services/runtime*.rs` | _TBD_ | _TBD_ | — | — | VAC already has runtime tab (#05) |
| `tui/src/services/autopilot*.rs` | _TBD_ | _TBD_ | — | — | VAC owns scheduler state |

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

1. ~~Does the donor palette widget assume the donor `AppState`?~~ **Resolved** — `helper_dropdown.rs` reads `AppState.input_state` only for the filtered helper list. A caller that passes a precomputed `Vec<ShellCommandSpec>` removes the coupling. Step 3 confirmed feasible.
2. **`stakai::Model` projection** — `model_switcher.rs` reads provider, cost, reasoning flag from the donor type. Two paths: (a) `VacModelView` adds the same fields and we copy the donor file with a type alias swap, or (b) we fork the donor file at copy time and replace the type. **Decision: pick (a) only if no donor field requires Stakpak-specific semantics (cost is per-token; provider is a string) — likely fine.** Confirm at Step 2 contract impl.
3. **Sessions list** — no dedicated donor module; lives inside `shortcuts_popup.rs`. Extracting the sessions section means we either copy the whole popup or surgically lift one render fn. Lean toward surgical.
4. `cli/` vs `crates/vac_cli/` — eventually one of them survives. Out of scope for Step 1.

## Changelog

- 2026-04-25 · skeleton landed; donor pinned at `2e75bd5`.
- 2026-04-25 · added `extraction shape` + `provenance` columns; added manager/worker reject rule.
- 2026-04-25 · Step 1 fill-in pass — 8 Phase-1 candidates classified. Net result: 2 ✅ as-is (`helper_dropdown`, `shell_popup`), 5 🟡 with-adapter (`commands`, `shortcuts_popup`, `approval_bar`, `model_switcher`, `plan`), 2 ❌ reject (`app`, `auto_approve`). Step 3 proof-of-life (command palette via `helper_dropdown`) confirmed feasible.
