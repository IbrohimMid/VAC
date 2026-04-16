# Milestone 3 Master Checklist

Dokumen ini menurunkan desain Milestone 3 menjadi checklist eksekusi yang siap dipakai di `main`, dengan status `PASS` / `PARTIAL` / `BLOCKER` per branch.

Milestone 3 tetap dibatasi pada:
- runtime/security/operator parity
- control-plane hardening
- execution boundary
- operator shell and background runtime
- MCP trust posture

Milestone 3 bukan:
- redesign planner
- redefinisi VIL
- import pola generic agent yang bertentangan dengan IR, validator, generated plumbing, semantic roles, atau zero-copy discipline

## Keputusan

Saya setuju dengan arah desain ini.

Berdasarkan audit codepath aktif di `main`, status branch plan yang realistis adalah:

| Branch | Nama | Status di `main` |
| --- | --- | --- |
| 3A | Isolation Foundation | BLOCKER |
| 3B | Trusted MCP | BLOCKER |
| 3C | Shell Active Path | BLOCKER |
| 3D | Runtime Jobs TUI | PARTIAL |
| 3E | Mode Matrix & Hardening | PARTIAL |

## Evidence Baseline di `main`

### PASS

- Background runtime sudah nyata:
  - `crates/vac_core/src/config.rs` memiliki `RuntimeConfig`
  - `crates/vac_runtime/src/jobs.rs`, `queue.rs`, `scheduler.rs`, `autopilot.rs` sudah hidup
  - `crates/vac_cli/src/commands/runtime.rs` sudah punya `status`, `jobs`, `cancel`, `retry`, `inspect`, `start`
  - `crates/vac_cli/src/commands/autopilot.rs` dan test `crates/vac_cli/tests/autopilot_controller_e2e.rs` sudah menutupi daemon/state path
- MCP baseline sudah nyata:
  - `crates/vac_tools/src/mcp/mod.rs` punya `McpServerConfig`
  - `crates/vac_tools/src/mcp/client.rs` sudah mendukung `stdio` dan `sse`
- TUI aktif Milestone 2 memang hidup:
  - `crates/vac_cli/src/tui/app/types.rs` punya `WorkbenchTab::{Approvals, Review, Sessions}`
  - `crates/vac_cli/src/tui/view.rs` sudah me-render workbench, approvals, review, sessions
- Ada donor shell/PTy yang bisa diangkat:
  - `crates/vac_cli/src/tui/services_stakpak_disabled/handlers/shell.rs`
  - `crates/vac_cli/src/tui/services_stakpak_disabled/shell_mode.rs`
  - `crates/vac_cli/src/tui/services_stakpak_disabled/shell_popup.rs`

### PARTIAL

- `OperatingMode` sudah ada, tetapi masih intent-only:
  - `crates/vac_runtime/src/executor.rs`
  - nilainya masih `MonitorOnly`, `SuggestOnly`, `PatchProposal`, `AutoFixLowRisk`
- Tool policy/risk baseline sudah ada:
  - `crates/vac_tools/src/router.rs`
  - `crates/vac_tools/src/registry.rs`
- TUI punya jejak shell preview, tetapi belum shell lifecycle aktif:
  - `crates/vac_cli/src/tui/app/types.rs` hanya menyimpan `shell_popup_visible` dan `shell_output`
  - `crates/vac_cli/src/tui/view.rs` hanya menampilkan preview dua baris terakhir pada operator panel

### BLOCKER

- Tidak ada execution boundary nyata:
  - `RuntimeConfig` hanya punya `enable`, `operating_mode`, `max_concurrent_jobs`
  - `crates/vac_tools/src/sandbox.rs` bukan container/isolation boundary; saat ini hanya wrapper boolean + command sanitization
- MCP trust posture belum aman:
  - `McpServerConfig` hanya `name`, `transport`, `env`
  - SSE client memakai `reqwest::Client::new()`
  - `McpProxyTool::trust_requirement()` hardcoded `"trusted"`
- Shell active path belum wired:
  - `crates/vac_cli/src/tui/app/events.rs` tidak punya event shell lifecycle
  - active `event_loop.rs` tidak memproses flow PTY/shell
  - `OutputEvent::ExecuteCommand` di `crates/vac_cli/src/tui/runner.rs` men-trigger task engine, bukan operator PTY session
- Runtime jobs belum surfaced di TUI:
  - tidak ada `WorkbenchTab::Runtime`
  - tidak ada input/output event runtime jobs

## Branch 3A — Isolation Foundation

**Status:** BLOCKER

### PASS di `main`

- Baseline runtime config sudah ada di `crates/vac_core/src/config.rs`
- Executor/autopilot path sudah ada di:
  - `crates/vac_runtime/src/executor.rs`
  - `crates/vac_runtime/src/autopilot.rs`
  - `crates/vac_cli/src/commands/runtime.rs`

### BLOCKER yang wajib ditutup

- Tambahkan `ExecutionEnvironment`
  - `Host`
  - `IsolatedInteractive`
  - `IsolatedBatch`
- Tambahkan `IsolationConfig` ke `RuntimeConfig`
  - `execution_environment`
  - `container_runtime`
  - `container_image`
  - `allowed_mounts`
  - `allowed_env`
  - `network_policy`
- Implement runner abstraction:
  - `HostRunner`
  - `ContainerRunner`
- Tambahkan CLI resmi:
  - `vac isolation run`
  - `vac isolation status`
  - `vac isolation logs`
- Tambahkan guard:
  - mount validator
  - env allowlist passthrough
  - tty vs non-tty
  - network deny/allow telemetry
- Integrasikan runtime/autopilot agar operator bisa memilih host vs isolated path

### File target

- `crates/vac_core/src/config.rs`
- `crates/vac_runtime/src/isolation.rs` baru
- `crates/vac_runtime/src/executor.rs`
- `crates/vac_runtime/src/autopilot.rs`
- `crates/vac_runtime/src/lib.rs`
- `crates/vac_cli/src/main.rs`
- `crates/vac_cli/src/commands/isolation.rs` baru
- `crates/vac_cli/src/commands/mod.rs`

### Exit gate

- VAC dapat dijalankan di host atau environment terisolasi nyata
- mount/env/network yang tidak diizinkan ditolak eksplisit
- mode eksekusi terlihat dari CLI dan state runtime
- production path tidak lagi host-only by default

## Branch 3B — Trusted MCP

**Status:** BLOCKER

### PASS di `main`

- `McpServerConfig` dan `McpClient` sudah ada
- Transport `stdio` dan `sse` sudah bekerja
- Proxy tool registration sudah ada

### BLOCKER yang wajib ditutup

- Perluas `McpServerConfig` dengan:
  - `trust_class`
  - `tls.ca_file`
  - `tls.client_cert_file`
  - `tls.client_key_file`
  - `tls.server_name`
  - `tls.require_mtls`
  - `approval_policy`
  - `allowed_in_modes`
- Tambahkan `McpTrustClass`
  - `LocalTrusted`
  - `RemoteVerified`
  - `RemoteUntrusted`
- Ganti `reqwest::Client::new()` dengan builder berbasis TLS policy
- Remote server tanpa trust config harus downgrade atau reject
- `McpProxyTool::trust_requirement()` harus mewarisi trust class server asal
- Tool router harus bisa membatasi tool exposure berdasarkan trust class dan mode
- Operator harus bisa melihat status:
  - trusted
  - verified
  - untrusted
  - tls error

### File target

- `crates/vac_tools/src/mcp/mod.rs`
- `crates/vac_tools/src/mcp/client.rs`
- `crates/vac_tools/src/router.rs`
- `crates/vac_tools/src/registry.rs`
- `crates/vac_core/src/config.rs`
- `crates/vac_cli/src/commands/config.rs`
- `docs/MCP_ACP_GAP_ANALYSIS.md`

### Exit gate

- remote MCP tidak otomatis dianggap trusted
- verified remote MCP bisa memakai TLS/mTLS config
- proxy tools mewarisi trust class asal
- trust failure surfaced jelas di operator path

## Branch 3C — Shell Active Path

**Status:** BLOCKER

### PASS di `main`

- Ada donor PTY/shell lifecycle yang cukup kaya di path disabled
- `AppState` aktif sudah punya baseline shell preview field
- `view.rs` aktif sudah punya operator panel yang bisa diperluas

### BLOCKER yang wajib ditutup

- Tambahkan event aktif:
  - `RunShellCommand`
  - `ShellOutput`
  - `ShellError`
  - `ShellCompleted`
  - `ShellWaitingForInput`
  - `ShellClear`
  - `ShellKill`
  - `ToggleShellMode`
  - `BackgroundShell`
- Tambahkan state aktif untuk shell session, bukan hanya `shell_output`
- Aktifkan renderer shell popup atau pane
- Integrasikan PTY session dengan event loop aktif
- Tambahkan background/refocus/kill lifecycle
- Hubungkan shell result ke tool result bridge bila dipakai oleh command-based flow

### File target

- `crates/vac_cli/src/tui/app/events.rs`
- `crates/vac_cli/src/tui/app/types.rs`
- `crates/vac_cli/src/tui/event.rs`
- `crates/vac_cli/src/tui/event_loop.rs`
- `crates/vac_cli/src/tui/view.rs`
- `crates/vac_cli/src/tui/runner.rs`
- `crates/vac_cli/src/tui/handlers/shell.rs` baru
- `crates/vac_cli/src/tui/handlers/mod.rs`
- `crates/vac_cli/src/tui/services/shell_mode.rs` baru, dipotong dari donor
- `crates/vac_cli/src/tui/services/shell_popup.rs` baru, dipotong dari donor

### Donor source yang dipakai

- `crates/vac_cli/src/tui/services_stakpak_disabled/handlers/shell.rs`
- `crates/vac_cli/src/tui/services_stakpak_disabled/shell_mode.rs`
- `crates/vac_cli/src/tui/services_stakpak_disabled/shell_popup.rs`

### Exit gate

- operator bisa menjalankan command interaktif dari TUI
- output mengalir live
- shell bisa dibackground dan difocus ulang
- interactive prompt yang minta input tetap jalan
- flow shell tidak lagi tersembunyi di donor path

## Branch 3D — Runtime Jobs TUI

**Status:** PARTIAL

### PASS di `main`

- Queue/job model sudah ada:
  - `crates/vac_runtime/src/jobs.rs`
  - `crates/vac_runtime/src/queue.rs`
- Runtime CLI operator control sudah ada:
  - `crates/vac_cli/src/commands/runtime.rs`
- Autopilot state sudah mengandung:
  - polling
  - executing
  - waiting approval
  - backoff
  - failed

### BLOCKER yang wajib ditutup

- Tambahkan `WorkbenchTab::Runtime`
- Tambahkan state:
  - `runtime_jobs`
  - `runtime_selected_idx`
  - `runtime_filter`
  - `runtime_detail`
  - `runtime_state_snapshot`
- Tambahkan output/input event:
  - list runtime jobs
  - inspect job
  - cancel job
  - retry job
  - refresh runtime state
- Render list dan detail panel di TUI
- Surface execution environment dan operating mode aktif di tab runtime

### File target

- `crates/vac_cli/src/tui/app/types.rs`
- `crates/vac_cli/src/tui/app/events.rs`
- `crates/vac_cli/src/tui/event.rs`
- `crates/vac_cli/src/tui/event_loop.rs`
- `crates/vac_cli/src/tui/view.rs`
- `crates/vac_cli/src/tui/runner.rs`
- `crates/vac_cli/src/tui/services.rs`
- `crates/vac_cli/src/commands/runtime.rs`

### Exit gate

- operator dapat melihat queue dari TUI
- cancel/retry/inspect bisa dilakukan dari TUI
- waiting approval dan backoff terlihat jelas
- operator tidak perlu membuka `.vac/autopilot.state` manual

## Branch 3E — Mode Matrix & Hardening

**Status:** PARTIAL

### PASS di `main`

- `OperatingMode` baseline sudah ada di `crates/vac_runtime/src/executor.rs`
- Risk/policy baseline sudah ada di `crates/vac_tools/src/router.rs`
- Runtime dan autopilot sudah punya test baseline:
  - `crates/vac_runtime/tests/queue_tests.rs`
  - `crates/vac_cli/tests/autopilot_status.rs`
  - `crates/vac_cli/tests/autopilot_controller_e2e.rs`
- Tool smoke test baseline sudah ada:
  - `crates/vac_tools/tests/tool_smoke.rs`

### BLOCKER yang wajib ditutup

- Pisahkan `OperatingMode` menjadi:
  - `TaskIntentMode`
  - `EnvironmentMode`
- Tambahkan policy matrix untuk:
  - allowed tools
  - allowed MCP classes
  - shell enabled/disabled
  - write capability
  - remote/network posture
- Tambahkan validator untuk kombinasi mode ilegal
- Surface mode aktif di header, runtime tab, dan status output
- Tambahkan regression track:
  - isolation tests
  - MCP trust tests
  - shell lifecycle tests
  - runtime jobs TUI tests
- Tambahkan operator docs:
  - runtime operating guide
  - isolation guide
  - trusted MCP guide
  - environment mode matrix

### File target

- `crates/vac_core/src/config.rs`
- `crates/vac_runtime/src/executor.rs`
- `crates/vac_runtime/src/autopilot.rs`
- `crates/vac_tools/src/router.rs`
- `crates/vac_tools/src/mcp/mod.rs`
- `crates/vac_tools/src/mcp/client.rs`
- `crates/vac_cli/src/tui/view.rs`
- `crates/vac_cli/tests/`
- `crates/vac_runtime/tests/`
- `crates/vac_tools/tests/`
- `docs/`

### Exit gate

- mode intent dan environment tidak lagi ambigu
- tool exposure berubah sesuai kombinasi mode
- remote/untrusted MCP tidak muncul pada mode yang melarangnya
- hard boundary utama punya regression coverage

## Hardening Track M3-F

Status hardening keseluruhan saat ini adalah **PARTIAL**.

Checklist wajib:
- Isolation tests:
  - denied mount
  - denied env
  - batch non-tty
  - interactive tty
- MCP trust tests:
  - local trusted
  - remote without trust config rejected
  - TLS failure surfaced
  - proxy tool inherits trust class
- Runtime operator tests:
  - jobs tab refresh
  - cancel/retry/inspect
  - waiting approval visibility
- Shell lifecycle tests:
  - output streaming
  - background/refocus
  - completion/kill
  - tool result bridging
- Docs:
  - operator guide
  - isolation guide
  - trusted MCP guide
  - mode matrix

## Dependency Order

Urutan branch yang paling aman:

1. `3A Isolation Foundation`
2. `3B Trusted MCP`
3. `3C Shell Active Path`
4. `3D Runtime Jobs TUI`
5. `3E Mode Matrix & Hardening`

Alasannya:
- `3A` dan `3B` menetapkan execution boundary dan trust boundary
- `3C` menetapkan operator shell parity
- `3D` membangun control-plane surface di atas runtime yang sudah nyata
- `3E` baru stabil jika boundary, trust, dan operator plane sudah fixed

## Definition of Done Milestone 3

Milestone 3 baru boleh dianggap selesai jika semua kondisi ini terpenuhi:

- VAC bisa berjalan di host atau isolated environment
- remote MCP tidak diperlakukan setara local MCP tanpa trust configuration
- shell/PTy operator flow aktif di TUI, bukan disabled donor
- runtime jobs visible dan operable dari TUI
- environment mode dan task intent mode terpisah jelas
- cancel/retry/inspect/background approval terlihat operator
- coverage ada untuk isolation, MCP trust, shell lifecycle, runtime jobs
- tidak ada perubahan yang melanggar prinsip VIL:
  - IR as truth
  - generated plumbing
  - semantic safety
  - zero-copy discipline

## Ringkasan Tegas

Checklist ini mengunci bahwa Milestone 3 bukan pekerjaan kosmetik TUI.

Milestone 3 adalah perubahan VAC dari:

> agent engine + TUI yang sudah usable

menjadi:

> VIL-native autonomous coding agent dengan runtime boundary, trust posture, shell operator flow, dan control plane yang layak production
