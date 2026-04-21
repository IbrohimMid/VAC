# Action Matrix

Every user-invocable intent in the TUI. Single source of truth for
`action_registry.rs`. Per-scope keybindings must be unique (enforced by
`action_spec_coverage` test added in Patch 2.2).

## Scope legend

- `G` Global (always available)
- `W/Input` Workspace → Input focus
- `W/Conv` Workspace → Conversation focus
- `W/Act` Workspace → Activity focus
- `WB/<Tab>` Workbench tab
- `OV/<Id>` Inside an overlay

## Keybinding normalization (Patch 4.3)

| Key | Meaning | Scope |
| --- | --- | --- |
| `Enter` | Primary / Confirm / Toggle-detail | All |
| `a` | Approve / Primary-affirmative | WB/Approvals |
| `x` | Reject / Destructive-primary | WB/Approvals |
| `r` | Refresh / Reload | WB/* |
| `Ctrl+R` | Resume checkpoint | WB/Sessions |
| `R` (shift) | Repair | WB/Vil |
| `Esc` | Close overlay / Cancel / Background | All |
| `Tab` / `Shift+Tab` | Cycle focus / tab | G |
| `Ctrl+P` | Command palette | G |
| `?` | Shortcuts | G |
| `/` | Slash / Helper dropdown | W/Input |
| `@` | At dropdown | W/Input |
| `!` | Shell | W/Input |
| `Ctrl+F` | File search | W/Input, WB/* |
| `Ctrl+C` | Cancel stream / Quit confirm | G |

## Action registry fields

Each row in `action_registry.rs` produces:

```rust
ActionSpec {
    id: ActionId,
    title: &'static str,
    description: &'static str,
    scope: ActionContext,
    keybindings: &'static [KeyChord],
    slash_aliases: &'static [&'static str],
    palette_visible: bool,
    footer_visible: bool,
    availability: fn(&AppState) -> bool,
    invoke: fn(&mut HandlerContext) -> Result<()>,
    activity_message: Option<fn(&AppState) -> String>,
}
```

## Registry coverage (to be filled as Patch 2.1 lands)

| ActionId | Scope | Keys | Slash | Palette | Footer |
| --- | --- | --- | --- | --- | --- |
| OpenCommandPalette | G | Ctrl+P | — | no | yes |
| OpenShortcuts | G | ? | /help | yes | yes |
| OpenFileSearch | G | Ctrl+F | /files | yes | yes |
| QuitApp | G | Ctrl+C×2 | /quit | yes | yes |
| CancelStream | G | Ctrl+C | — | no | yes |
| SwitchModel | G | — | /model | yes | no |
| SwitchProfile | G | — | /profile | yes | no |
| SwitchIsolation | G | — | /isolation | yes | no |
| SwitchRulebook | G | — | /rulebook | yes | no |
| ApproveCurrent | WB/Approvals | a / Enter | — | no | yes |
| RejectCurrent | WB/Approvals | x | — | no | yes |
| RefreshList | WB/* | r | — | no | yes |
| ResumeCheckpoint | WB/Sessions | Ctrl+R | /resume | yes | yes |
| OpenReviewItem | WB/Review | Enter | — | no | yes |
| OpenPlanReview | WB/Plan | Enter | /plan | yes | yes |
| RepairVil | WB/Vil | R | /repair | yes | yes |
| ConfirmAskUser | OV/AskUser | Enter | — | no | yes |
| CloseOverlay | OV/* | Esc | — | no | yes |

*(non-exhaustive — expanded as each patch lands.)*

## Conflict rules

1. Within a single scope, no two entries may share a keybinding.
2. Across overlapping scopes (e.g. `G` and any `W/*`), `G` yields to the
   more specific scope.
3. Slash aliases are globally unique.

## Tests that enforce this doc

- `action_spec_coverage` — every `ActionId` has a row here.
- `no_keybinding_collisions_per_scope` — asserts rule 1.
- `slash_aliases_unique` — asserts rule 3.

## Mouse click dispatch surfaces (PR-T16)

`handlers::mouse::dispatch_click` walks a fixed priority cascade and returns
`true` on the first region it handles. Regions are populated by the view
render pass; regions left empty are simply skipped. `mouse` tests in
`handlers/mouse.rs` assert the cascade order and row-region precedence
over the body fallback.

| Priority | Surface | Populator (field) | On click |
| --- | --- | --- | --- |
| 1 | Banner dismiss button | `banner_dismiss_region` | clear active banner |
| 2 | Banner body chip | `banner_click_regions` | open banner target |
| 3 | Workbench tab header | `workbench_tab_regions` | switch `workbench_tab` + focus Workbench |
| 4 | Task tray overlay row | `task_tray_row_regions` | select tray row index |
| 5 | Side panel header | `side_panel_header_areas` (via `input_core`) | collapse/expand section |
| 6 | Side panel row | `side_panel_row_areas` (via `input_core`) | invoke row action |
| 7 | Review file row | `review_file_row_regions` | select path + focus Workbench + tab Review |
| 8 | Approvals pane row | `approvals_row_regions` | select idx + focus Workbench + tab Approvals |
| 9 | VIL issue row | `vil_issue_row_regions` | select idx + focus Workbench + tab Vil |
| 10 | Workbench body fallback | `workbench_body_region` | focus Workbench (no tab switch) |

Row-region hits (7–9) fire BEFORE the body fallback (10), so clicking on a
row does not get re-interpreted as a generic focus grab.

## Keybinding overrides (PR-T19)

User keybindings live at `.vac/keybindings.toml` and are loaded at startup
by `services::keybindings_loader`. The merged chord keymap is installed
via `keybindings_runtime::install_global_keymap` and consulted by
`lookup_override_global` before the built-in matcher.

Resolution order for any `KeyEvent`:

1. User override (`lookup_override_global`) — returns an `InputEvent` if
   the chord is bound to an `ActionId`.
2. Per-scope built-in binding (`ActionSpec.keybindings` within the active
   `ActionContext`).
3. Global built-in binding (`ActionSpec` with `scope = G`).

Loader errors (missing action id, unknown chord, per-scope collision) are
surfaced as warning banners at startup — the user keeps a working default
keymap rather than a silent half-override.

As of PR-T19 R1, two additional diagnostic classes are surfaced the same
way:

- **Non-reachable override** — user bound a slash-only / workbench-only
  action that can never be reached via a single global chord. The
  binding is ignored and the user is told which action / chord pair was
  dropped.
- **Duplicate chord** — user bound the same chord to multiple actions.
  Last-writer wins in `chord_to_action`, but all competing bindings are
  surfaced so the user can edit the TOML.

See `ChordKeymap::skipped_bindings()` and `ChordKeymap::conflicts()` for
the runtime API; the bootstrap in `event_loop.rs` wires both into
`BannerStyle::Warning / BannerSeverity::Suggested`.

## Command recorder / replay (PR-T18)

The TUI ships a JSON-lines input recorder and a replay driver that reads
the same format. Exposed via `vac interactive --record <dir>` and
`vac interactive --replay <file>` (mutually exclusive).

Full user-facing recipes — capturing a session, shipping demos with the
repo, attaching a minimal repro to a bug report, and the list of
`RecordedInput` variants that are / are not serialized — live in
[`docs/tui/recorder_replay.md`](./recorder_replay.md).

## Mouse dispatch surface list (PR-T16)

Left-click routing flows through `handlers::mouse::dispatch_click` after
banner / side-panel handling in `input_core`. The view render pass
populates per-surface click regions on `AppState`, and `dispatch_click`
matches them in the order below (first hit wins):

| Surface                         | Region field on `AppState`     | Behaviour                                             |
| ------------------------------- | ------------------------------ | ----------------------------------------------------- |
| Workbench tab strip             | `workbench_tab_regions`        | Switch tab + focus Workbench                          |
| Task tray overlay rows          | `task_tray_row_regions`        | Select tray row (only while TaskTray overlay active)  |
| Review file rows                | `review_file_row_regions`      | Select path + switch to Review tab                    |
| Approvals rows                  | `approvals_row_regions`        | Select idx + switch to Approvals tab                  |
| Vil issue rows                  | `vil_issue_row_regions`        | Select idx + switch to Vil tab                        |
| Sessions rows (R5)              | `sessions_row_regions`         | Select idx + switch to Sessions tab                   |
| Workbench body focus fallback   | `workbench_body_region`        | Grab focus without changing the active tab            |

Banner dismissal and banner action buttons run earlier inside
`handlers::input_core::handle_mouse_drag_start` via `banner_dismiss_region`
/ `banner_click_regions`; those never reach `dispatch_click`.

Residual surfaces that are **not yet wired** (tracked as R5 backlog):

- Command / shortcuts popup rows (`shortcuts_popup`) — currently
  scroll-only, no per-entry click regions emitted.
- Side-panel collapse / header toggle regions — handled inline in
  `input_core`, not surfaced via `*_regions`.
- Scrollbar handle jump-to-offset for long panes.
- Status line chips (model / profile / ruleset).

Each of these can be closed by adding a `*_regions` field, populating it
during the relevant render, and appending a branch to `dispatch_click`
mirroring the pattern above. E2E coverage belongs in the
`pr_t16_mouse_dispatch_e2e` module in `crates/vac_cli/tests/tui_flows.rs`.
