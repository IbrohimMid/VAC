# Command recorder / replay (PR-T18)

The VAC TUI can record every user input event to a JSON-lines file and
replay that file deterministically on any machine. This is the minimum
surface needed for:

- **Demo recipes** — shipping reproducible walkthroughs with the repo.
- **Bug reports** — attaching a minimal input trace to a GitHub issue.
- **Regression tests** — pinning interactive flows against snapshots
  (see `tests/tui_flows.rs` for the harness-driven variant).

> This document is the companion to `services/recorder.rs` and the
> `--record` / `--replay` flags on `vac interactive`. It intentionally
> shows *full shell recipes* rather than just API-level fragments —
> auditor residual R2 asked for user-facing examples, not library docs.

## Where recordings live

Unless overridden with a path, recordings land under:

```
.vac/recordings/<unix_ms>.jsonl
```

The directory is managed by the recorder itself:

- files rotate at **10 MB** per file (`DEFAULT_MAX_BYTES`);
- the **20 most recent** files are kept (`DEFAULT_MAX_FILES`), older
  files are pruned on rotation.

The `.jsonl` format is one `RecordedLine` per line:

```json
{"ts_ms":1713700000000,"event":{"kind":"Key","code":"Char('p')","modifiers":2}}
{"ts_ms":1713700000050,"event":{"kind":"MouseDragStart","col":12,"row":4}}
{"ts_ms":1713700000120,"event":{"kind":"Resize","cols":120,"rows":40}}
```

Replay is purely event-stream driven: the rest of app state is a pure
function of those inputs, so replaying the same file against the same
build produces the same final frame.

## Recipe 1 — record an interactive session

```bash
# Starts the TUI and writes every key/mouse/resize/paste into
# .vac/recordings/<unix_ms>.jsonl while you use it normally.
vac interactive --record .vac/recordings

# Do whatever you want to capture, then quit with Ctrl+Q as usual.
# The .jsonl file is flushed on shutdown.
ls -1t .vac/recordings | head -1
```

Typical use: capturing a 30-second demo of your approval flow before
filing a bug.

## Recipe 2 — replay a prior recording

```bash
# Pass the exact file you want to replay. Terminal input is ignored
# while the replay stream is draining; once exhausted, input falls
# back to the live terminal.
vac interactive --replay .vac/recordings/1713700000000.jsonl
```

Note the two flags are mutually exclusive (`clap` rejects
`--record` + `--replay` together).

## Recipe 3 — ship a demo with the repo

```bash
# 1. Capture the canonical demo once, locally.
vac interactive --record examples/demos
mv "$(ls -1t examples/demos | head -1)" examples/demos/onboarding.jsonl

# 2. Commit the .jsonl.
git add examples/demos/onboarding.jsonl
git commit -m "docs(demo): add onboarding replay"

# 3. Reviewers can re-run the exact same session with:
vac interactive --replay examples/demos/onboarding.jsonl
```

Because the file is plain JSON-lines, it diffs cleanly in code review
and is small enough to include in bug reports (typical 30-second
sessions are a few KB).

## Recipe 4 — attach a minimal repro to a bug report

```bash
# Point --record at /tmp so the file is disposable.
vac interactive --record /tmp/vac-repro
# Reproduce the bug, then Ctrl+Q.
latest=$(ls -1t /tmp/vac-repro | head -1)
gzip -k "/tmp/vac-repro/$latest"
echo "Attach /tmp/vac-repro/${latest}.gz to the GitHub issue."
```

## What is (and isn't) recorded

`RecordedInput` only covers events the user produced at the terminal:

| Variant           | Recorded? | Notes                                    |
|-------------------|-----------|------------------------------------------|
| `Key`             | ✅        | `code` is the `crossterm::KeyCode` Debug |
| `MouseDragStart`  | ✅        | column + row in terminal cells           |
| `MouseDrag`       | ✅        | emitted while a drag is active           |
| `MouseDragEnd`    | ✅        | one-shot at release                      |
| `Resize`          | ✅        | terminal dimensions                      |
| `Paste`           | ✅        | full pasted text                         |
| Backend events    | ❌        | LLM responses, tool results, session IDs |
| Wall-clock timers | ❌        | replay is event-driven, not time-driven  |

Backend events are intentionally excluded — they are a *function* of the
recorded inputs given the same build and profile, so including them
would bloat the file and create a consistency hazard.

## Gotchas

- Replays bind to the **build you ran the session on**. If the
  `ActionSpec` table or input router changed between recording and
  replay, some events may no longer map to the same action. Pin the
  commit SHA in demo recipes that ship with the repo.
- `--record` creates the directory if missing but does not clean up old
  files outside its own rotation policy. If you want a throwaway
  directory, point it at `/tmp/...`.
- Paste events serialize the full text. Don't record sessions where
  you'd paste secrets — treat recording files as you'd treat a scroll
  buffer.

## Programmatic use

For tests and tooling, skip the CLI and drive `Recorder` / `Replay`
directly:

```rust
use vac_tui_runtime::services::recorder::{Recorder, RecorderConfig, RecordedInput, Replay};

let cfg = RecorderConfig {
    dir: temp_dir.path().into(),
    max_bytes: 1024 * 1024,
    max_files: 4,
};
let mut rec = Recorder::open(cfg)?;
rec.record(RecordedInput::Key { code: "Char('q')".into(), modifiers: 2 })?;
let path = rec.current_path().to_path_buf();
drop(rec); // flushes

for line in Replay::open(&path)? {
    println!("{:?} @ {}", line.event, line.ts_ms);
}
```

This is exactly what the `recorder_replay_roundtrip_preserves_event_stream`
test in `crates/vac_cli/tests/integration_events.rs` exercises.
