# VAC Deployment Guide

This guide covers shipping VAC (`vac` CLI binary + `vac_tui_runtime`)
to operator workstations and headless CI agents.

## 1. Supported platforms

| Target | Status | Notes |
|---|---|---|
| Linux x86_64 (glibc ≥ 2.31) | tier-1 | Primary development target |
| Linux aarch64 | tier-1 | Same build flags as x86_64 |
| macOS x86_64 + arm64 | tier-2 | Kitty graphics probe requires recent terminal |
| Windows x86_64 (MSYS / WT) | tier-3 | TUI fully supported; some paths assume POSIX perms |

## 2. Binary install

### 2.1 Cargo from source

```bash
git clone https://github.com/IbrohimMid/VAC.git
cd VAC
cargo build --release -p vac_cli
install -m 0755 target/release/vac ~/.local/bin/
```

Cold build is 10–18 min on a 16-core laptop; warm incremental is
under 1 min. The workspace shares one `target/` directory — do NOT
override `--target-dir`, that defeats the incremental cache.

### 2.2 Feature flags

`vac_cli` exposes two operator-facing features:

| Feature | Effect |
|---|---|
| `tui2` | Transplanted Stakpak TUI under a feature flag (experimental) |
| `signal-rewind` | Enables `vac signal tail` — SQLite-backed signal persistence via `vac_signal/rewind` |

Enable with `cargo build --release -p vac_cli --features signal-rewind`.

## 3. First-run setup on an operator workstation

```bash
# Point the CLI at your project.
cd ~/src/my-project
vac init           # writes .vac/ scaffolding (config, memdir, autopilot)

# Optional: boot the interactive TUI cockpit.
vac interactive

# Optional: drive a single submit through the session-engine path.
vac session-run "investigate module X" --provider mock
```

`vac init` creates:

```
.vac/
  config.toml        # LLM provider, rulebook paths, isolation policy
  autopilot.toml     # cron schedules (F6.1), modes
  memory/            # vac_memory memdir (F4)
    active/
    archived/
    team/
  sessions/          # session-engine transcripts (F2)
  backups/           # reversible-edit snapshots (F6.4)
```

Everything under `.vac/` is safe to gitignore; `memory/team/` is
the one directory you explicitly *do* want committed (team-shared
knowledge).

## 4. Headless / CI deployment

`vac` runs cleanly without a TTY for CI jobs:

```bash
vac session-run "audit changelog against rulebook" \
    --provider mock \
    --no-trajectory      # ephemeral transcript, auto-cleaned (F8.3)
```

or a richer one-shot:

```bash
vac run "write tests for auth module" \
    --approve            # auto-approve tools that match the allow-list
    --engine session     # route through vac_session_engine (durable
                         # transcript under .vac/sessions/)
```

The `--engine` flag (or `VAC_ENGINE=session` env) selects the engine
path. Legacy default stays as `VacEngine::run_task_with_approvals`
during the M2 coexistence window; the session path uses
`vac_session_engine::submit_one` and produces the same event stream.

CI agents typically want `signal-rewind` OFF (SQLite adds ~2 MB) and
a scoped `.vac/config.toml` that pins the provider + approvals.

### 4.1 Preview subcommands (ultraplan Wave 3)

These land with Wave 3 of `ultraplan-vac-product.md`. Listed here so
operators know the surfaces to expect:

```bash
# P1 — Proactive assistant. Watches the signal pipeline for build
# failures + test regressions; surfaces tasks as suggestions.
vac assistant [--session <id>]

# P2 — Remote deep planner. Offloads long planning to a remote
# model; returns a plan file and optional patch set.
vac plan "<prompt>" [--remote <endpoint>]
vac plan apply <plan-id>    # stage plan patches as a changeset
```

Until Wave 3 ships, these subcommands exit with a `not yet
implemented` message.

## 5. Observability

Every submit produces a transcript JSONL under `.vac/sessions/`. To
inspect one run:

```bash
vac decisions    # dump AgentDecision records from the trace
vac eval         # score decisions vs outcomes
```

The signal pipeline (F3 onwards) exposes:

```bash
vac signal list                # per-stream summary
vac signal tail <key> --follow # live tail, rewind-store backed
```

OpenTelemetry export lives behind `--otel-endpoint`; see
`CLAUDE.md` for the full flag list.

## 6. Upgrade path

`.vac/` layout is forward-compatible via `vac migrate`:

```bash
vac migrate       # no-op on matching versions; auto-backs up on change
```

Session transcripts are the durable contract — every refactor across
Fase 2–10 preserves the append-only JSONL shape. Drivers can replay a
transcript from any VAC version ≥ 0.1 without schema conversion.

## 7. Security surface

| Surface | Default | Hardening |
|---|---|---|
| Tool execution | prompt per call | `--approve` for CI; rulebook-scoped allow/deny |
| File edits | backup snapshot before write (F6.4) | Restore with `vac restore --backup <hash>` |
| MCP servers | disabled until registered in config | F5 state machine enforces `connected` before tool dispatch |
| Remote bridge (ACP) | off until `vac acp serve` | F5 `vac_bridge` validates handshake + protocol version band |
| Secrets | `vac_tools::privacy::PrivacyVault` | Redaction layer between tool output and transcript |

## 8. Testing discipline

Workspace rules (see `CLAUDE.md`):

- `cargo test` is blocked by a PreToolUse hook. Use **`cargo nextest run`**
  — ~3× faster, one process per test, full parallelism.
- Scope to the crate you edited: `cargo nextest run -p vac_session_engine`.
- For SLA tests flaking on noisy CI, export
  `VAC_SLA_TOLERANCE=3` to triple every budget uniformly without
  changing the asserted ratios.

Validate the workspace compiles (cheap, ~10–30 s on a warm cache):

```bash
cargo check --workspace --tests
```

## 9. Troubleshooting

**Cold start latency > 5 s.** Expected on first boot; subsequent
boots are <500 ms (hydration deferred per F3.4). If persistent,
inspect `.vac/sessions/` size and prune manually:

```bash
# Remove all but the most recent 50 session transcripts:
ls -t .vac/sessions/*.jsonl | tail -n +51 | xargs -r rm
```

**Transcript JSONL corruption.** `TranscriptWriter` fsync-per-append
+ per-session mutex (F2) prevents interleaving. If you still see
partial lines, check disk space and inode limits; the writer
refuses to silently drop.

**Memory consolidator never fires.** Check
`.vac/memory/.consolidator.stamp` — the cooldown gate (default 30
min, F4.3) requires elapsed time. Force the next run by removing
the stamp:

```bash
rm -f .vac/memory/.consolidator.stamp
```

Then trigger a submit that would otherwise pass the gate; the
consolidator re-evaluates on the next session close.

**Rate-limit banner stuck.** The 10-message bank (F7.6) rotates
only on explicit `next_message()` calls from the driver. If the
banner text doesn't change, check that the engine is actually
cycling — it rotates on new throttle events, not on a timer.

**Restoring a reversible edit.** The backup module (F6.4) snapshots
files to `.vac/backups/<content>_<path>.snap` before edits. Use
`vac restore <file>` to roll back the most recent snapshot for a
given path. See `vac restore --help` for the live flag list.
