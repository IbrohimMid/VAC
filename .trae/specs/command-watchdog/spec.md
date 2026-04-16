# Spec: Long-Running Command Watchdog

## Why

Ketika agent menjalankan command (bash, test, build, dll) yang hang atau berjalan terlalu lama, operator tidak punya visibilitas dan tidak bisa intervensi. Akibatnya:

- Development time terbuang menunggu command yang tidak akan selesai
- Tidak ada cara untuk kill command tanpa keluar dari TUI
- Tidak ada informasi tentang command mana yang menjadi culprit
- Agent bisa stuck selamanya tanpa feedback ke operator

Ini adalah **P0 developer experience issue** — terbukti dari kasus nyata di mana `cargo test` hang selama 5+ jam tanpa intervensi.

## What Changes

Tambahkan **Command Watchdog** di VAC runtime yang:

1. Memantau durasi setiap command yang dijalankan agent
2. Menampilkan popup TUI saat command melebihi threshold
3. Memberi operator pilihan: kill, wait, atau find culprit
4. Menampilkan informasi diagnostik (PID, command, durasi, output tail)

## Impact

- Affected code: `vac_tools` (bash tool execution), `vac_cli` (TUI popup), `vac_runtime` (watchdog timer)
- Affected specs: Tool Execution, Approval Flow, Runtime Observability

---

## Requirements

### Requirement: Command Duration Watchdog

Sistem SHALL mendeteksi command yang berjalan melebihi threshold dan menginformasikan operator.

#### Scenario: Command melebihi warning threshold

- **WHEN** agent menjalankan command yang berjalan > `warn_threshold` (default: 30s)
- **THEN** TUI menampilkan toast warning: `"Command '{name}' running for {duration}s"`
- **AND** activity log diupdate dengan status `SlowCommand`

#### Scenario: Command melebihi critical threshold

- **WHEN** agent menjalankan command yang berjalan > `critical_threshold` (default: 120s)
- **THEN** TUI menampilkan popup modal dengan informasi:
  - Command yang sedang berjalan (nama + args truncated)
  - Durasi berjalan
  - PID proses
  - Tail 10 baris output terakhir
- **AND** popup menawarkan 3 pilihan:
  - `[K] Kill command` — terminate proses, lanjut ke command berikutnya
  - `[W] Wait longer` — dismiss popup, set threshold +60s
  - `[D] Find culprit` — kill command, buka shell untuk investigasi

#### Scenario: Operator memilih Kill

- **WHEN** operator memilih Kill dari watchdog popup
- **THEN** proses di-terminate (SIGTERM → SIGKILL setelah 3s)
- **AND** agent menerima error result untuk command tersebut
- **AND** agent dapat melanjutkan atau retry dengan pendekatan berbeda

#### Scenario: Operator memilih Find Culprit

- **WHEN** operator memilih Find Culprit
- **THEN** command di-kill
- **AND** TUI membuka shell popup dengan context:
  - PID yang baru di-kill
  - Working directory
  - Suggested commands: `ps aux`, `lsof`, `strace`

---

## Implementation Plan

### Branch 7A: Watchdog Core

**Scope**: Runtime watchdog timer + event pipeline

**Tasks**:
1. Tambahkan `CommandWatchdog` struct di `vac_tools/src/bash.rs`:
   - Spawn tokio task yang monitor durasi command
   - Emit `InputEvent::CommandSlowWarning { pid, command, duration_secs }` saat warn threshold
   - Emit `InputEvent::CommandCriticalTimeout { pid, command, duration_secs, output_tail }` saat critical threshold
2. Tambahkan events ke `InputEvent` enum
3. Tambahkan `watchdog_warn_secs: u64` dan `watchdog_critical_secs: u64` ke config (default 30s/120s)
4. Wire events ke TUI via existing input channel

**Exit criteria**:
- Command yang hang > 30s emit warning event
- Command yang hang > 120s emit critical event
- Events sampai ke TUI

### Branch 7B: Watchdog TUI Popup

**Scope**: TUI popup + operator actions

**Tasks**:
1. Tambahkan `WatchdogPopup` state ke `AppState`:
   ```rust
   pub watchdog_popup: Option<WatchdogInfo>
   
   pub struct WatchdogInfo {
       pub pid: u32,
       pub command: String,
       pub duration_secs: u64,
       pub output_tail: Vec<String>,
   }
   ```
2. Handle `CommandCriticalTimeout` event → set `watchdog_popup`
3. Handle `CommandSlowWarning` event → push toast
4. Render `render_watchdog_popup` di `view.rs`:
   - Modal overlay dengan border merah
   - Command info + duration + PID
   - Output tail (scrollable)
   - 3 action buttons: `[K]ill  [W]ait  [D]iagnose`
5. Handle keyboard: `k` = kill, `w` = wait, `d` = diagnose
6. Tambahkan `InputEvent::WatchdogKill(pid)`, `WatchdogWait`, `WatchdogDiagnose(pid)`
7. Wire kill → send SIGTERM ke PID via `OutputEvent::KillCommand(pid)`
8. Wire diagnose → open shell popup dengan context

**Exit criteria**:
- Popup muncul saat critical timeout
- Kill menghentikan proses dan dismiss popup
- Wait dismiss popup dan extend threshold
- Diagnose kill + buka shell context

### Branch 7C: Watchdog Tests + Config

**Scope**: Tests + configurable thresholds

**Tasks**:
1. Unit tests untuk watchdog timer logic
2. Integration test: command yang hang → event emitted
3. Tambahkan ke `vac.toml` config:
   ```toml
   [watchdog]
   warn_secs = 30
   critical_secs = 120
   enabled = true
   ```
4. Behavioral tests untuk popup actions

**Exit criteria**:
- Thresholds configurable via vac.toml
- Tests cover warn/critical/kill/wait/diagnose paths

---

## Priority

**P0** — ini langsung mempengaruhi development velocity setiap kali agent menjalankan command yang bermasalah.

## Placement di Roadmap

Masuk sebagai **Milestone 2.5** — antara Milestone 2 (Operator UX Parity) dan Milestone 3 (Runtime Isolation):

```
Milestone 1: Stabilize Core UX ✅
Milestone 2: Operator UX Parity ✅ (in progress)
Milestone 2.5: Command Watchdog (NEW - P0)
Milestone 3: Runtime Isolation + MCP Hardening
```

Alasan dipisah dari Milestone 2: scope berbeda (runtime monitoring vs TUI UX), dan urgency-nya cukup tinggi untuk tidak menunggu Milestone 3.

## Default Thresholds

| Threshold | Default | Rationale |
|-----------|---------|-----------|
| warn | 30s | Cukup lama untuk command normal, tapi masih wajar untuk build |
| critical | 120s | 2 menit — hampir pasti ada masalah |
| kill_grace | 3s | SIGTERM → SIGKILL grace period |

Thresholds bisa di-override per-command via tool metadata di masa depan.
