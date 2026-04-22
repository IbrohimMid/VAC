# Action Matrix

Every user-invocable intent in the TUI. Single source of truth for
`action_registry.rs`. Per-scope keybindings are unique (enforced by
`action_spec_coverage` test in `action_registry.rs`).

## Scope legend

| Code | Meaning |
|------|---------|
| `G` | Global — always available |
| `W/Input` | Workspace → Input focus |
| `W/Conv` | Workspace → Conversation focus |
| `W/Act` | Workspace → Activity focus |
| `WB/*` | Workbench tab (any) |
| `WB/Review` | Workbench Review tab |
| `WB/Approvals` | Workbench Approvals tab |
| `WB/Sessions` | Workbench Sessions tab |
| `WB/Runtime` | Workbench Runtime tab |
| `WB/Vil` | Workbench VIL tab |
| `WB/Vwfd` | Workbench VWFD tab |
| `OV/<Id>` | Inside a specific overlay |

## Canonical keybindings

| Key | Meaning | Scope |
|-----|---------|-------|
| `Enter` | Primary / Confirm / Toggle-detail | All |
| `a` | Approve / Primary-affirmative | WB/Approvals |
| `x` | Reject / Destructive-primary | WB/Approvals |
| `r` | Refresh / Reload | WB/* |
| `Ctrl+R` | Resume checkpoint | WB/Sessions |
| `R` (shift) | Repair | WB/Vil |
| `Esc` | Close overlay / Cancel / Background | All |
| `Tab` / `Shift+Tab` | Cycle focus / next tab | G |
| `Ctrl+P` | Command palette | G |
| `?` | Shortcuts reference popup | G |
| `/` | Slash command / helper dropdown | W/Input |
| `@` | At-mention / context chip | W/Input |
| `!` | Shell launcher | W/Input |
| `Ctrl+F` | File search picker | W/Input, WB/* |
| `Ctrl+T` | Task tray overlay | G |
| `Ctrl+C` | Cancel stream (once) / Quit confirm (twice, 2s window) | G |
| `Alt+H` | vil-expr type-info popup | W/Input (vil-expr payload) |
| `m` | Mode toggle (VWFD execution mode cycle) | WB/Vwfd |
| `c` | Collapse/expand section | Side panel header |

## Action registry fields

Each `ActionSpec` in `action_registry.rs`:

```rust
ActionSpec {
    id:               ActionId,
    title:            &'static str,
    description:      &'static str,
    scope:            ActionContext,
    keybindings:      &'static [KeyChord],
    slash_aliases:    &'static [&'static str],
    palette_visible:  bool,
    footer_visible:   bool,
    availability:     fn(&AppState) -> bool,
    invoke:           fn(&mut HandlerContext) -> Result<()>,
    activity_message: Option<fn(&AppState) -> String>,
}
```

## Global actions

| ActionId | Keys | Slash | Description |
|----------|------|-------|-------------|
| OpenCommandPalette | Ctrl+P | — | Open command palette |
| OpenShortcutsPopup | ? | — | Show keybindings reference |
| OpenTaskTray | Ctrl+T | `/tasks` | Open background task tray |
| CancelStream | Ctrl+C (×1) | — | Cancel active LLM stream |
| Quit | Ctrl+C (×2, 2s) | `/exit`, `/quit` | Exit TUI |

## Input actions

| ActionId | Keys | Slash | Description |
|----------|------|-------|-------------|
| SubmitMessage | Enter | — | Send message to agent |
| OpenFilePicker | Ctrl+F | — | Open fuzzy file picker |
| OpenModelSwitcher | @ prefix | — | Switch active LLM model |
| OpenProfileSwitcher | — | `/profile` | Switch profile |
| OpenIsolationSwitcher | — | `/isolation` | Switch execution environment |
| OpenRulebookSwitcher | — | `/rulebook` | Select active rulebooks |
| ClearConversation | — | `/clear` | Clear conversation history |
| VilExprTypeHelp | Alt+H | — | Show vil-expr identifier type info (vil-expr payload active) |
| RunShell | ! prefix | `/shell` | Launch shell session |

## Workbench — Approvals tab

| ActionId | Keys | Description |
|----------|------|-------------|
| ApproveCurrentTool | a | Approve selected tool call |
| RejectCurrentTool | x | Reject selected tool call |
| ApproveAllPending | A | Approve all pending tool calls |
| ToggleToolDetail | Enter | Expand/collapse tool call arguments |

## Workbench — Review tab

| ActionId | Keys | Description |
|----------|------|-------------|
| ReviewScrollDown | j / ↓ | Scroll diff down |
| ReviewScrollUp | k / ↑ | Scroll diff up |
| ReviewNextFile | n | Jump to next changed file |
| ReviewPrevFile | N | Jump to previous changed file |
| ReviewToggleSplit | s | Toggle split/unified diff view |

## Workbench — Sessions tab

| ActionId | Keys | Description |
|----------|------|-------------|
| ResumeSession | Ctrl+R | Resume selected session from checkpoint |

## Workbench — VIL tab

| ActionId | Keys | Description |
|----------|------|-------------|
| VilRepairIssue | R | Trigger repair proposal for selected issue |
| VilNextIssue | j / ↓ | Next issue |
| VilPrevIssue | k / ↑ | Previous issue |
| VilToggleCanonical | c | Toggle canonical-only filter |

## Workbench — VWFD tab

| ActionId | Keys | Description |
|----------|------|-------------|
| VwfdNodeUp | k / ↑ | Move tree selection up |
| VwfdNodeDown | j / ↓ | Move tree selection down |
| VwfdNodeExpand | Enter / → | Expand selected node |
| VwfdJumpToSource | Enter (on handler) | Jump to handler source location |
| VwfdModeToggle | m | Cycle execution mode preview (Native → WASM → Sidecar) |
