# Tasks: Command Watchdog

- [ ] Branch 7A: Watchdog Core
  - [ ] Tambahkan `CommandWatchdog` di `vac_tools/src/bash.rs`
  - [ ] Tambahkan `InputEvent::CommandSlowWarning` dan `CommandCriticalTimeout`
  - [ ] Wire watchdog events ke TUI input channel
  - [ ] Tambahkan config fields `watchdog_warn_secs` / `watchdog_critical_secs`

- [ ] Branch 7B: Watchdog TUI Popup
  - [ ] Tambahkan `WatchdogInfo` struct dan `watchdog_popup: Option<WatchdogInfo>` ke AppState
  - [ ] Handle `CommandCriticalTimeout` → set popup, `CommandSlowWarning` → toast
  - [ ] Render `render_watchdog_popup` di view.rs (modal merah, command info, output tail, 3 actions)
  - [ ] Handle `k`/`w`/`d` keyboard actions
  - [ ] Wire Kill → `OutputEvent::KillCommand(pid)` → SIGTERM/SIGKILL
  - [ ] Wire Diagnose → kill + open shell popup dengan context

- [ ] Branch 7C: Tests + Config
  - [ ] Unit tests watchdog timer
  - [ ] Behavioral tests popup actions (kill/wait/diagnose)
  - [ ] Config via `vac.toml` `[watchdog]` section
  - [ ] Integration test: hang command → event emitted

# Dependencies

- Branch 7A tidak bergantung pada Milestone 2
- Branch 7B bergantung pada Branch 7A
- Branch 7C bergantung pada Branch 7A + 7B
