# Runtime Operating Guide

Dokumen ini merangkum surface operator Milestone 3 yang terkait dengan runtime, isolation boundary, MCP trust posture, dan mode matrix.

## Runtime Modes

`[runtime]` sekarang memisahkan dua dimensi:

- `task_intent_mode`
  - `monitor-only`
  - `suggest-only`
  - `patch-proposal`
  - `auto-fix-low-risk`
- `environment_mode`
  - `host`
  - `isolated`
  - `trusted-networked`
  - `restricted-offline`

Selain itu, boundary aktual dijelaskan oleh:

- `execution_environment`
  - `host`
  - `isolated_interactive`
  - `isolated_batch`

## Contoh Konfigurasi

```toml
[runtime]
enable = true
task_intent_mode = "patch-proposal"
environment_mode = "isolated"
execution_environment = "isolated_interactive"
network_policy = "restricted_offline"
container_runtime = "docker"
container_image = "ghcr.io/your-org/vac-runtime:latest"
allowed_mounts = ["."]
allowed_env = ["KILO_API_KEY"]
max_concurrent_jobs = 2
```

## CLI Operator Surface

Runtime:

```bash
vac runtime status
vac runtime jobs
vac runtime inspect <job-id>
vac runtime cancel <job-id>
vac runtime retry <job-id>
vac runtime start
```

Isolation:

```bash
vac isolation status
vac isolation logs
vac isolation run -- bash -lc "pwd && ls"
```

Autopilot:

```bash
vac autopilot up
vac autopilot status
vac autopilot down
```

Jika `runtime.execution_environment` diset ke mode isolated, `vac runtime start` dan `vac autopilot up` akan membungkus eksekusi melalui container runtime yang dikonfigurasi.
Jika `execution_environment = "isolated_interactive"`, `/shell` di TUI juga akan dijalankan melalui container runtime yang sama. `isolated_batch` menolak shell operator interaktif.

## MCP Tool Approval & Parallelism

`[[mcp_servers]]` mendukung konfigurasi tool approval dan paralelisme. Anda dapat mengatur persetujuan secara global per-server atau secara spesifik per-tool.

Contoh konfigurasi:

```toml
[[mcp_servers]]
name = "my-tools"
transport.type = "stdio"
transport.command = "my-tool-server"

# Izinkan / larang paralel tool calls untuk server ini
supports_parallel_tool_calls = false

# Mode approval default untuk semua tool di server ini ("prompt", "approve", "deny")
default_tools_approval_mode = "prompt"

# Override spesifik per-tool
[mcp_servers.tools.sensitive_tool]
approval_mode = "prompt"

[mcp_servers.tools.safe_tool]
approval_mode = "approve"
```

## MCP Trust Classes

`[[mcp_servers]]` sekarang mendukung:

- `local_trusted`
- `remote_verified`
- `remote_untrusted`

Perilaku default:

- `stdio` tanpa `trust_class` -> `local_trusted`
- `sse` tanpa `trust_class` -> `remote_untrusted`

Contoh remote verified:

```toml
[[mcp_servers]]
name = "remote-knowledge"
transport.type = "sse"
transport.url = "https://mcp.example.com"
trust_class = "remote_verified"
allowed_in_modes = ["trusted-networked"]

[mcp_servers.tls]
ca_file = "/etc/ssl/custom-ca.pem"
server_name = "mcp.example.com"
```

Catatan:

- `remote_verified` mewajibkan URL `https`
- `remote_untrusted` akan diblok di `trusted-networked` dan `isolated` untuk tool proxy yang tidak dipercaya
- `restricted-offline` menolak tool MCP remote sama sekali

## Environment Matrix

| Environment Mode | Local MCP | Remote Verified MCP | Remote Untrusted MCP | Shell |
| --- | --- | --- | --- | --- |
| `host` | allow | allow | allow | allow |
| `isolated` | allow | allow | deny | allow |
| `trusted-networked` | allow | allow | deny | allow |
| `restricted-offline` | allow | deny | deny | allow |

Catatan shell:

- `execution_environment = "host"` -> `/shell` memakai host PTY
- `execution_environment = "isolated_interactive"` -> `/shell` memakai PTY dalam container
- `execution_environment = "isolated_batch"` -> `/shell` ditolak

## TUI Operator Surface

Workbench sekarang memiliki tab `Runtime`.

Di tab ini operator bisa:

- melihat queue jobs
- melihat snapshot state autopilot
- `r` untuk refresh
- `c` untuk cancel job terpilih
- `t` untuk retry job terpilih

Shell operator flow:

- `/shell <cmd>` menjalankan PTY command
- `/shell-bg` membackground session aktif
- `/shell-focus` memfokuskan kembali session
- `/shell-kill` menghentikan session
- saat popup shell aktif, input box VAC akan mengirim input ke PTY pada saat `Enter`

## Audit Notes

- `vac runtime status --format json` menampilkan execution environment dan isolation config
- `vac isolation logs` menampilkan denied mount/env serta lifecycle container wrapper
- proxy MCP tools mewarisi trust class dari server asal
