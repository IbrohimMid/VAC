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
