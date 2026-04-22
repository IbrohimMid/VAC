# PRD — Runtime & Scheduler

**Feature Area:** `crates/vac_runtime`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

`vac_runtime` provides the background execution infrastructure: a job queue, a task dependency graph, a cron scheduler, an autopilot daemon, and an isolation manager. It runs independently of the TUI and is accessible via `vac runtime` and `vac autopilot` commands.

---

## Job Queue

All agent tasks are enqueued as `Job` records:

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Unique job identifier |
| `kind` | JobKind | Task type (see below) |
| `status` | JobStatus | Current execution state |
| `created_at` | Timestamp | Enqueue time |
| `started_at` | Option<Timestamp> | Execution start |
| `completed_at` | Option<Timestamp> | Terminal state time |

### JobKind

| Variant | Description |
|---------|-------------|
| `RunTask { description }` | One-shot agent task |
| `VilDev` | `vil dev` streaming process |
| `Autopilot` | Background autopilot task |
| `Scheduled { cron }` | Time-triggered task |

### JobStatus

```
Pending → Running → Completed
                  → Failed { reason }
                  → Cancelled
```

---

## Task Graph

Complex tasks are modeled as a DAG of `TaskNode` entries:

| Field | Purpose |
|-------|---------|
| `id` | Node identifier |
| `dependencies` | IDs of nodes that must complete first |
| `token_budget` | Per-node context budget |
| `retry_count` | Current retry number |
| `max_retries` | Retry cap |
| `approval_policy` | `ReadOnly` / `SafeEdit` / `RequireAll` / `AutoApprove` |

The Planner agent populates the task graph; the runtime scheduler executes ready nodes in parallel up to `max_executors`.

### Approval Policies

| Policy | Behavior |
|--------|---------|
| `ReadOnly` | No writes allowed; all file mutations auto-rejected |
| `SafeEdit` | Writes to files already in the changeset auto-approved |
| `RequireAll` | Every tool call requires explicit approval |
| `AutoApprove` | All calls auto-approved (headless) |

---

## Scheduler

The `Scheduler` polls the job queue at a configurable interval:

```
Idle → Polling → Executing → WaitingApproval
     ↑___________________________|
```

- In `WaitingApproval`, the scheduler pauses execution of the blocked node and continues independent nodes
- On autopilot, the scheduler remains in `Polling` state between tasks

---

## Cron Scheduler

Time-based triggers enqueue jobs at the specified schedule:

```toml
[[runtime.cron]]
schedule = "0 9 * * 1-5"          # 09:00 Mon–Fri
task = "Run daily lint pass"
profile = "strict"
```

Cron entries are validated on `vac init` and `vac config set`. Invalid cron expressions are rejected.

---

## Autopilot Daemon

`vac autopilot up` starts a background daemon that:

1. Watches `.vac/inbox/` for task files dropped by external tools or scripts
2. Runs queued tasks sequentially (or concurrently if configured)
3. Reports status via `vac autopilot status`
4. Integrates with the cron scheduler for time-based tasks

The daemon is a supervised process; it restarts automatically on crash (systemd-style supervision optional).

```bash
vac autopilot up                   # start daemon
vac autopilot down                 # stop daemon
vac autopilot status               # daemon state + queue depth
vac autopilot run "Fix lint"       # submit task to daemon queue
```

---

## Isolation Manager

`IsolationManager` configures the execution environment for every tool call:

| Mode | Description |
|------|-------------|
| `Host` | Tools run directly on the host filesystem |
| `Container` | Tools run inside a Docker/OCI container |
| `Interactive` | Interactive shell session (pty) |
| `Batch` | Non-interactive, captured output |

### Container Configuration

```toml
[runtime.isolation]
mode = "container"
image = "vastar/vil-runtime:latest"
allowed_mounts = ["/workspace"]
allowed_env = ["HOME", "RUST_LOG"]
network_policy = "none"            # none | host | bridge
```

`vac isolation doctor` verifies the container runtime is available and the configured image can be pulled.

---

## Runtime Inspection

```bash
vac runtime status                 # queue depth, mode, environment
vac runtime jobs                   # list all jobs (with status)
vac runtime inspect <job-id>       # job detail + task graph node states
vac runtime cancel <job-id>        # cancel a running or pending job
vac runtime retry <job-id>         # retry a failed job
```

The TUI Runtime tab provides the same view with live updates.

---

## vil dev Integration

When `vac vil dev` runs, the runner spawns the `vil dev` subprocess and emits `RunnerEvent` values into the job queue:

| Event | Action |
|-------|--------|
| `Started { pid }` | Creates a `JobKind::VilDev` job, stores PID |
| `Stdout(line)` | Appends to output ring buffer (max 500 lines) |
| `Stderr(line)` | Appends to output ring buffer + activity log |
| `Checkpoint { session_id }` | Records checkpoint ID |
| `Exited { code, signal }` | Updates job to Completed/Failed |
| `Error(msg)` | Marks job Failed, logs to activity |

The job appears in the TUI task tray with Running / Exited / Error state badges.
