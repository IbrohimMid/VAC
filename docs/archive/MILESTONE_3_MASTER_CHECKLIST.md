# Milestone 3 Master Checklist — Revised

Dokumen ini adalah revisi akurat terhadap state `main` saat ini.
Versi sebelumnya stale karena dibuat sebelum implementasi branch 3A–3E selesai masuk ke `main`.

Milestone 3 tetap dibatasi pada:
- runtime/security/operator parity
- control-plane hardening
- execution boundary
- operator shell and background runtime
- MCP trust posture

Bukan:
- redesign planner atau redefinisi VIL
- import pola generic agent yang bertentangan dengan IR, validator, generated plumbing, semantic roles, atau zero-copy discipline

---

## Status Branch — Ringkasan Eksekutif

| Branch | Nama                    | Status Aktual     | Gap Tersisa                                      |
|--------|-------------------------|-------------------|--------------------------------------------------|
| 3A     | Isolation Foundation    | **PASS kuat**     | Header TUI tidak menampilkan environment mode    |
| 3B     | Trusted MCP             | **PARTIAL kuat**  | Trust surface/observability ke operator belum ada|
| 3C     | Shell Active Path       | **PARTIAL kuat**  | PTY aktif, lifecycle tests & hardening kurang    |
| 3D     | Runtime Jobs TUI        | **PASS**          | Done                                             |
| 3E     | Mode Matrix & Hardening | **PASS kuat**     | Header TUI tidak menampilkan mode aktif          |
| 3F     | Verification            | **PARTIAL**       | Shell lifecycle tests & operator docs kurang     |

---

## Branch 3A — Isolation Foundation

**Status: PASS kuat — 1 PARTIAL tersisa**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| `ExecutionEnvironment` enum (Host/IsolatedInteractive/IsolatedBatch) | `crates/vac_core/src/config.rs:427` | ada |
| `NetworkPolicy` enum (Inherit/RestrictedOffline/TrustedNetworked) | `crates/vac_core/src/config.rs:434` | ada |
| `task_intent_mode` + `environment_mode` dipisah di `RuntimeConfig` | `crates/vac_core/src/config.rs:399` | ada |
| `RuntimeConfig.validate()` dengan cross-check isolation↔container_image dan mode combinations | `crates/vac_core/src/config.rs:477` | ada |
| `allowed_mcp_classes()`, `shell_execution_mode()`, `write_capability()` sebagai policy helpers | `crates/vac_core/src/config.rs:539` | ada |
| `IsolationManager` dengan `resolve_mounts`, `filter_allowed_env`, `build_container_command` | `crates/vac_runtime/src/isolation.rs` | ada |
| `isolation.log_path()` + `append_log()` — denial events dicatat | `crates/vac_runtime/src/isolation.rs:54` | ada |
| `spawn_background()` untuk isolated batch execution | `crates/vac_runtime/src/isolation.rs:248` | ada |
| `vac isolation status` CLI | `crates/vac_cli/src/commands/isolation.rs:6` | ada |
| `vac isolation logs` CLI | `crates/vac_cli/src/commands/isolation.rs:53` | ada |
| `vac isolation run` CLI | `crates/vac_cli/src/commands/isolation.rs:65` | ada |
| Tests: denied mount, network none, interactive shell spec | `crates/vac_runtime/src/isolation.rs:278` | ada |
| Autopilot meneruskan `environment_mode` + `execution_environment` ke state file | `crates/vac_runtime/src/autopilot.rs` | ada |

### PARTIAL — gap yang wajib ditutup

- **`render_header()` tidak menampilkan `environment_mode`, `execution_environment`, atau `task_intent_mode`**
  - File: `crates/vac_cli/src/tui/view.rs:277`
  - Header sekarang hanya menampilkan: `session`, `model`, `perm AUTO/MANUAL`, `approvals`, `review`
  - Gap: operator tidak tahu VAC berjalan di mode apa tanpa membuka Runtime tab atau file state
  - Fix: tambahkan span `env:<environment_mode>` dan `intent:<task_intent_mode>` ke header, sumber dari `runtime_state_snapshot` atau config load awal

---

## Branch 3B — Trusted MCP

**Status: PARTIAL kuat — schema selesai, operator surface belum**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| `McpTrustClass` enum (LocalTrusted/RemoteVerified/RemoteUntrusted) | `crates/vac_tools/src/mcp/mod.rs:36` | ada |
| `McpTlsConfig` struct (ca_file, client_cert/key, server_name, require_mtls) | `crates/vac_tools/src/mcp/mod.rs:44` | ada |
| `McpServerConfig` extended dengan trust_class + tls + approval_policy + allowed_in_modes | `crates/vac_tools/src/mcp/mod.rs:8` | ada |
| `effective_trust_class()` — SSE remote → RemoteUntrusted by default | `crates/vac_tools/src/mcp/mod.rs:121` | ada |
| `is_allowed_in_mode()` | `crates/vac_tools/src/mcp/mod.rs:131` | ada |
| `proxy_trust_requirement()` — proxy tools mewarisi trust dari server asal | `crates/vac_tools/src/mcp/mod.rs:111` | ada |
| SSE client menggunakan `build_sse_client()` berbasis trust class, bukan hardcoded `Client::new()` | `crates/vac_tools/src/mcp/client.rs:90` | ada |
| Engine init memfilter MCP servers dengan `is_allowed_in_mode()` sebelum connect | `crates/vac_core/src/engine.rs:156` | ada |
| Tests: stdio → LocalTrusted, SSE → RemoteUntrusted by default | `crates/vac_tools/src/mcp/mod.rs:140` | ada |

### PARTIAL — gap yang wajib ditutup

1. **Trust status tidak terlihat dari CLI**
   - Tidak ada `vac mcp list` atau `vac mcp status` yang menampilkan server name, trust class, connection state, dan TLS posture
   - Fix: tambahkan subcommand `vac mcp list` di `crates/vac_cli/src/commands/` dengan output: name, transport, trust_class, allowed_in_modes, tls enabled

2. **Trust status tidak terlihat dari TUI**
   - Tidak ada badge atau panel yang menunjukkan MCP servers aktif beserta trust class-nya
   - Fix: tambahkan entri MCP di Runtime pane atau operator activity stream dengan label `trusted`/`verified`/`untrusted`/`tls-error`

3. **TLS failure tidak surfaced ke operator**
   - Jika `build_sse_client()` gagal karena TLS error, error hanya masuk log trace, tidak muncul di TUI atau CLI output
   - Fix: pastikan TLS error dari `McpClient::connect()` muncul sebagai toast atau activity item di TUI, dan sebagai error output di CLI

---

## Branch 3C — Shell Active Path

**Status: PARTIAL kuat — PTY aktif, hardening belum**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| Shell state di `AppState` (shell_popup_visible, shell_output, active_shell_command, shell_waiting_for_input, shell_backgrounded, shell_exit_code, shell_last_error) | `crates/vac_cli/src/tui/app/types.rs:292` | ada |
| `InputEvent::ShellStarted/ShellOutput/ShellError/ShellCompleted/ShellWaitingForInput` | `crates/vac_cli/src/tui/app/events.rs:28` | ada |
| `BackgroundShell`, `FocusShell`, `ShellKill` events | `crates/vac_cli/src/tui/app/events.rs:123` | ada |
| `render_shell_popup()` aktif di `view.rs` | `crates/vac_cli/src/tui/view.rs:48` | ada |
| Shell event handling di `event_loop.rs` (BackgroundShell, FocusShell, ShellKill) | `crates/vac_cli/src/tui/event_loop.rs:905` | ada |
| `OutputEvent::ExecuteCommand` → `run_pty_command()` aktif di runner | `crates/vac_cli/src/tui/runner.rs:344` | ada |
| `build_interactive_shell_spec()` dipakai untuk `IsolatedInteractive` | `crates/vac_cli/src/tui/runner.rs:359` | ada |
| `IsolatedBatch` menolak shell secara eksplisit dengan error message | `crates/vac_cli/src/tui/runner.rs:373` | ada |
| ShellOutput/Error/Completed/WaitingForInput diforward ke TUI events | `crates/vac_cli/src/tui/runner.rs:400` | ada |
| Footer shell di view menampilkan state shell active/background/completed | `crates/vac_cli/src/tui/view.rs:1318` | ada |
| Operator panel menampilkan input box info untuk PTY | `crates/vac_cli/src/tui/view.rs` | ada |

### PARTIAL — gap yang wajib ditutup

1. **Lifecycle tests tidak ada**
   - Test yang ada di `event_loop.rs:2656` hanya cover `shell_popup_visible = true`, bukan PTY streaming lifecycle
   - Fix: tambahkan integration tests untuk: output streaming, background/refocus cycle, ShellCompleted → cleanup state, ShellKill → state reset

2. **Stdin forwarding ke PTY belum dikonfirmasi end-to-end**
   - `shell_waiting_for_input` diset saat event `ShellWaitingForInput`, tapi path dari user input textarea ke PTY stdin perlu dikonfirmasi wired
   - Fix: verifikasi `event_loop.rs` meneruskan input ke PTY stdin handle saat `shell_popup_visible && shell_waiting_for_input`

3. **Shell hasil (exit code, output) tidak dijembatani ke tool result**
   - `ShellCompleted(i32)` diterima dan disimpan ke `shell_exit_code`, tapi tidak ada bridge ke `ToolResult` untuk command-based flows
   - Fix: opsional untuk MVP, tapi dokumentasikan sebagai known gap

---

## Branch 3D — Runtime Jobs TUI

**Status: PASS — branch paling matang**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| `WorkbenchTab::Runtime` ada dan bisa dicapai via `next()` | `crates/vac_cli/src/tui/app/types.rs:118` | ada |
| `runtime_jobs`, `runtime_selected_idx`, `runtime_filter`, `runtime_detail_scroll`, `runtime_state_snapshot` di `AppState` | `crates/vac_cli/src/tui/app/types.rs:328` | ada |
| `OutputEvent::ListRuntimeJobs`, `CancelRuntimeJob`, `RetryRuntimeJob` | `crates/vac_cli/src/tui/app/events.rs:175` | ada |
| `InputEvent::SetRuntimeJobs`, `SetRuntimeState` | `crates/vac_cli/src/tui/app/events.rs:26` | ada |
| `render_runtime_pane()` aktif dan merender queue/job counts, autopilot mode, task_intent_mode, environment_mode, execution_environment, state, last_error, current_job, WaitingApproval, Backoff until | `crates/vac_cli/src/tui/view.rs:1013` | ada |
| Keyboard: j/k navigasi, c=cancel, r=retry, i=inspect | `crates/vac_cli/src/tui/event_loop.rs:583` | ada |
| Polling `load_runtime_jobs` di runner | `crates/vac_cli/src/tui/runner.rs:498` | ada |
| `AutopilotStateFile` loaded dan disimpan ke `runtime_state_snapshot` | `crates/vac_cli/src/tui/runner.rs:517` | ada |
| Footer shortcuts untuk Runtime tab | `crates/vac_cli/src/tui/view.rs:1440` | ada |

### Gap kecil (non-blocking)

- End-to-end test cancel/retry dari TUI belum ada (hanya unit state mutation tests)

---

## Branch 3E — Mode Matrix & Hardening

**Status: PASS kuat — 1 PARTIAL kecil tersisa**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| `TaskIntentMode` dan `EnvironmentMode` sebagai tipe terpisah | `crates/vac_runtime/src/executor.rs:9` | ada |
| `OperatingMode` sebagai type alias ke `TaskIntentMode` untuk backward compat | `crates/vac_runtime/src/executor.rs:80` | ada |
| `allowed_mcp_classes()` per environment mode | `crates/vac_core/src/config.rs:539` | ada |
| `shell_execution_mode()` per execution environment | `crates/vac_core/src/config.rs:547` | ada |
| `write_capability()` per task intent mode | `crates/vac_core/src/config.rs:555` | ada |
| Validation kombinasi ilegal (isolated↔host, trusted-networked↔restricted-offline) | `crates/vac_core/src/config.rs:498` | ada |
| `ToolContext.with_environment_mode()` — mode diteruskan ke tool context | `crates/vac_runtime/src/autopilot.rs:486` | ada |
| `is_allowed_in_mode()` dipakai di engine init untuk filter MCP servers | `crates/vac_core/src/engine.rs:156` | ada |

### PARTIAL — gap yang wajib ditutup

- **Header TUI tidak menampilkan mode aktif** (sama dengan gap 3A)
  - `render_header()` tidak menampilkan `task_intent_mode` atau `environment_mode`
  - Ini satu fix yang menutup PARTIAL di 3A sekaligus 3E

---

## Branch 3F — Verification & Docs

**Status: PARTIAL**

### PASS (dikonfirmasi di `main`)

| Item | File | Bukti |
|------|------|-------|
| Isolation tests: denied mount, network none, interactive shell spec | `crates/vac_runtime/src/isolation.rs:278` | ada |
| MCP trust tests: stdio → LocalTrusted, SSE → RemoteUntrusted | `crates/vac_tools/src/mcp/mod.rs:140` | ada |
| Config validation tests: illegal mode combinations | `crates/vac_core/tests/config_contract.rs` | ada |
| Runtime Jobs TUI state mutation tests | `crates/vac_cli/src/tui/event_loop.rs:2631` | ada |
| Docs: `docs/runtime_operating_guide.md` | ada | ada |

### PARTIAL — gap yang wajib ditutup

1. **Shell lifecycle tests tidak ada**
   - Tidak ada test untuk: PTY output streaming, background/refocus cycle, ShellCompleted state cleanup, ShellKill
   - Prioritas: P0 setelah stdin forwarding dikonfirmasi (lihat 3C gap 2)

2. **MCP trust CLI/TUI surface tests tidak ada**
   - Setelah `vac mcp list` diimplementasi (3B gap 1), perlu test coverage
   - Prioritas: P1

3. **Docs operator masih kurang**
   - `docs/runtime_operating_guide.md` ada tapi belum dikonfirmasi mencakup: isolation guide, trusted MCP guide, environment mode matrix
   - Prioritas: P1

---

## Prioritas Kerja Saat Ini

Urutan yang paling efisien berdasarkan state `main` aktual:

### P0 — 1 fix, 2 file

**Expose runtime mode di header TUI**

- Edit `crates/vac_cli/src/tui/view.rs` fungsi `render_header()` (baris 277)
- Tambahkan span untuk `environment_mode` dan `task_intent_mode` dari `state.runtime_state_snapshot`
- Menutup PARTIAL di 3A dan 3E sekaligus

### P0 — MCP trust surface

**Tambah `vac mcp list` CLI**

- Buat `crates/vac_cli/src/commands/mcp.rs`
- Wire di `crates/vac_cli/src/commands/mod.rs` dan `main.rs`
- Output: name, transport type, trust_class, allowed_in_modes, tls enabled

**Surface trust ke TUI**

- Tambahkan MCP server list dengan trust badge ke Runtime pane atau sebagai activity item saat `McpClient::connect()` berhasil/gagal

### P1 — Shell lifecycle tests

- Tambahkan tests di `crates/vac_cli/tests/` atau `crates/vac_cli/src/tui/` untuk PTY lifecycle
- Perlu mock atau test harness untuk `run_pty_command`

### P1 — Konfirmasi stdin forwarding ke PTY

- Verifikasi `event_loop.rs` meneruskan user input ke PTY stdin saat `shell_popup_visible && shell_waiting_for_input`
- Tambahkan test jika belum

### P2 — Operator docs

- Lengkapi/verifikasi `docs/runtime_operating_guide.md` mencakup: isolation guide, trusted MCP guide, environment mode matrix

---

## Definition of Done Milestone 3

Milestone 3 baru boleh dianggap **COMPLETE** jika semua kondisi berikut terpenuhi:

- [x] VAC bisa berjalan di host atau isolated environment
- [x] remote MCP tidak diperlakukan setara local MCP tanpa trust configuration
- [x] MCP servers difilter berdasarkan `allowed_in_modes` sebelum connect
- [x] shell/PTY operator flow aktif di TUI, bukan disabled donor
- [x] runtime jobs visible dan operable dari TUI
- [x] environment mode dan task intent mode terpisah jelas di config dan executor
- [x] cancel/retry/inspect/background approval terlihat operator dari TUI
- [x] coverage ada untuk isolation boundary, MCP trust defaults, config validation
- [ ] **environment_mode dan task_intent_mode terlihat di header TUI**
- [ ] **`vac mcp list` atau ekuivalen tersedia untuk operator**
- [ ] **TLS/trust failure surfaced ke operator (TUI toast atau CLI error)**
- [ ] **shell lifecycle tests ada (streaming, background, kill)**
- [ ] tidak ada perubahan yang melanggar prinsip VIL: IR as truth, generated plumbing, semantic safety, zero-copy discipline

---

*Direvisi berdasarkan audit `main` aktual — menggantikan versi checklist pertama yang stale.*
