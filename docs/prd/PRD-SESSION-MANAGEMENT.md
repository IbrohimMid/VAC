# PRD — Session Management

**Feature Area:** `crates/vac_session_control`, `crates/vac_changeset`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

Every agent task runs inside a named session. Sessions are checkpointed continuously so they can be resumed after interruption, replayed for audit, or exported for handoff.

---

## Session Lifecycle

```
Created → Running → Checkpointing → Completed
                  ↘             ↗
                   Interrupted → Resumed
```

Sessions reach `Completed` when the task reaches a terminal state (success or failure). `Interrupted` sessions can always be resumed via `vac resume` or `vac interactive --resume <id>`.

---

## SessionSnapshot

A snapshot captures metadata at a point in time:

| Field | Type | Description |
|-------|------|-------------|
| `session_id` | UUID | Unique session identifier |
| `schema_version` | u32 | Schema version for migration |
| `model` | String | LLM model used |
| `profile` | Option<String> | Configuration profile name |
| `task_count` | u32 | Number of completed steps |
| `token_usage` | TokenUsage | Input + output tokens consumed |
| `file_count` | u32 | Files modified in this session |
| `tui_focus` | Option<TabFocus> | Last active workbench tab |
| `user_metadata` | HashMap | Arbitrary user-supplied labels |
| `created_at` | Timestamp | Session start |
| `updated_at` | Timestamp | Last snapshot write |

Snapshots are stored as JSON at `.vac/sessions/<session_id>.json`.

---

## Checkpoints

A checkpoint is a snapshot taken at a specific step boundary:

- Written automatically every N steps (configurable, default 5)
- Written explicitly before any destructive tool call
- Written on clean shutdown
- Stored at `.vac/checkpoints/<session_id>/<checkpoint_id>.json`

On resume, the swarm reloads the latest checkpoint and replays any approved steps since then.

---

## Schema Migration

Snapshot files are versioned. When VAC detects an older schema version on load, it applies migrations in sequence (v0 → v1 → current). Migration is non-destructive: the original file is backed up before upgrade.

---

## Changeset Tracking (`vac_changeset`)

Every file touched during a session is tracked with a per-file lifecycle:

| State | Meaning |
|-------|---------|
| `Created` | File did not exist before the session |
| `Modified` | File existed and was changed |
| `Removed` | File was deleted by the agent |
| `Reverted` | File was restored to its pre-session state |

The changeset is persisted with the session and drives:

- **Review tab diff** — shows only files in the changeset
- **`vac restore`** — restores a single file to its pre-session state using the snapshot journal
- **Export bundles** — includes the changeset manifest

---

## File Restore

`vac restore <path>` walks the snapshot journal to find the pre-agent content of a file and writes it back:

```bash
vac restore src/auth.rs                          # restore to pre-session state
vac restore src/auth.rs --checkpoint <id>        # restore to specific checkpoint
```

This is a scoped undo: only the named file is reverted. The session and other files are unaffected.

---

## Session Listing

`vac interactive` (Sessions tab) and `vac resume --latest` both query the session store:

- Sessions sorted by `updated_at` descending
- Fuzzy search by task description or session ID
- Date filter (today / last 7 days / all)
- Stale sessions (not updated in > 30 days) shown with a visual indicator

---

## Export & Import

Sessions can be exported as a self-contained bundle:

```bash
vac export <session-id> --format vac-cbor          # compact binary
vac export <session-id> --format bundle-json --sign # human-readable, COSE-signed
```

Bundles include: snapshot, changeset manifest, approval records, trajectory traces, and the full conversation log.

```bash
vac import <bundle-file>
vac import <bundle-file> --require-signed   # reject unsigned bundles
vac import <bundle-file> --redact-secrets   # strip secrets on import
```

---

## JSONL Session Recorder

When the TUI recorder is active, every input event and rendered output frame is appended to a `.jsonl` file. This is separate from the snapshot system — it captures the full interaction log for replay:

```bash
vac interactive --replay .vac/sessions/recording.jsonl
```

Replay mode is read-only; no agent calls are made.
