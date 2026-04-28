use super::*;
use tempfile::TempDir;

fn sample_events() -> Vec<(RecordedInput, u128)> {
    vec![
        (
            RecordedInput::Key {
                code: "Enter".into(),
                modifiers: 0,
            },
            100,
        ),
        (
            RecordedInput::Key {
                code: "a".into(),
                modifiers: 0,
            },
            200,
        ),
        (RecordedInput::MouseDragStart { col: 5, row: 10 }, 300),
        (
            RecordedInput::Paste {
                text: "hello world".into(),
            },
            400,
        ),
        (
            RecordedInput::Resize {
                cols: 120,
                rows: 40,
            },
            500,
        ),
    ]
}

#[test]
fn round_trip_recording_produces_identical_state() {
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();
    let events = sample_events();
    for (ev, ts) in &events {
        rec.record_at(ev.clone(), *ts).unwrap();
    }
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    let replayed: Vec<RecordedLine> = Replay::open(&path).unwrap().collect();
    assert_eq!(replayed.len(), events.len());
    for (i, line) in replayed.iter().enumerate() {
        assert_eq!(line.ts_ms, events[i].1, "timestamp drift on entry {i}");
        assert_eq!(line.event, events[i].0, "event drift on entry {i}");
    }
}

#[test]
fn rotation_keeps_20_files() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("recordings");
    // Tiny per-file cap so every event triggers a rotation.
    let cfg = RecorderConfig::new(&dir)
        .with_max_bytes(32)
        .with_max_files(20);
    let mut rec = Recorder::open(cfg).unwrap();
    // Write enough events to force >20 rotations.
    for i in 0..50 {
        let ev = RecordedInput::Key {
            code: format!("K{i}"),
            modifiers: 0,
        };
        rec.record_at(ev, 1_000 + i as u128).unwrap();
    }
    rec.flush().unwrap();
    drop(rec);

    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        files.len() <= 20,
        "rotation must keep at most 20 files, found {}",
        files.len()
    );
    // Must actually have rotated at least once.
    assert!(files.len() > 1, "expected rotation to produce >1 file");
}

#[test]
fn rotation_prunes_oldest_first() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("recordings");
    let cfg = RecorderConfig::new(&dir)
        .with_max_bytes(32)
        .with_max_files(3);
    let mut rec = Recorder::open(cfg).unwrap();
    for i in 0..10 {
        let ev = RecordedInput::Key {
            code: format!("K{i}"),
            modifiers: 0,
        };
        rec.record_at(ev, 2_000 + i as u128).unwrap();
    }
    rec.flush().unwrap();
    drop(rec);

    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .filter(|n| n.ends_with(".jsonl"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 3, "must keep exactly max_files");
    // Highest ordinals survive — oldest dropped first.
    let last_ordinals: Vec<u32> = files
        .iter()
        .filter_map(|n| {
            n.strip_suffix(".jsonl")
                .and_then(|s| s.rsplit_once('-'))
                .and_then(|(_, o)| o.parse().ok())
        })
        .collect();
    assert!(last_ordinals.windows(2).all(|w| w[0] < w[1]));
    let first_ordinal = *last_ordinals.first().unwrap();
    assert!(
        first_ordinal >= 1,
        "oldest file with ordinal 0 must have been pruned, got {first_ordinal}"
    );
}

#[test]
fn replay_skips_corrupt_lines() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mixed.jsonl");
    let good = serde_json::to_string(&RecordedLine {
        ts_ms: 1,
        event: RecordedInput::Key {
            code: "X".into(),
            modifiers: 0,
        },
    })
    .unwrap();
    let torn = "{not valid json";
    std::fs::write(&path, format!("{good}\n{torn}\n{good}\n")).unwrap();

    let replayed: Vec<_> = Replay::open(&path).unwrap().collect();
    assert_eq!(replayed.len(), 2, "torn line must be skipped silently");
}

#[test]
fn empty_file_replay_yields_no_events() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("empty.jsonl");
    std::fs::write(&path, "").unwrap();
    let replayed: Vec<_> = Replay::open(&path).unwrap().collect();
    assert!(replayed.is_empty());
}

#[test]
fn consecutive_resizes_coalesce_to_last_value() {
    // PR-T18 R4: a window-drag emits dozens of Resize events per
    // second. The recorder must keep only the last one in the burst
    // so the replay file stays legible.
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();
    for (cols, rows, ts) in [(80, 24, 100), (90, 26, 110), (100, 28, 120), (120, 40, 130)] {
        rec.record_at(RecordedInput::Resize { cols, rows }, ts)
            .unwrap();
    }
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    let replayed: Vec<RecordedLine> = Replay::open(&path).unwrap().collect();
    assert_eq!(replayed.len(), 1, "burst must coalesce to one line");
    assert_eq!(replayed[0].ts_ms, 130, "ts must track the last sample");
    assert_eq!(
        replayed[0].event,
        RecordedInput::Resize {
            cols: 120,
            rows: 40
        },
        "value must be the last sample"
    );
}

#[test]
fn non_resize_after_resize_flushes_pending_in_order() {
    // After a Resize burst, the next non-Resize event must be preceded
    // by the pending Resize so replay sees the same ordering the user
    // experienced live.
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();
    rec.record_at(RecordedInput::Resize { cols: 80, rows: 24 }, 100)
        .unwrap();
    rec.record_at(
        RecordedInput::Resize {
            cols: 100,
            rows: 30,
        },
        150,
    )
    .unwrap();
    rec.record_at(
        RecordedInput::Key {
            code: "Enter".into(),
            modifiers: 0,
        },
        200,
    )
    .unwrap();
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    let replayed: Vec<RecordedLine> = Replay::open(&path).unwrap().collect();
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].ts_ms, 150);
    assert!(matches!(
        replayed[0].event,
        RecordedInput::Resize {
            cols: 100,
            rows: 30
        }
    ));
    assert!(matches!(replayed[1].event, RecordedInput::Key { .. }));
}

#[test]
fn recorder_writes_schema_header_on_open() {
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    // The first line on disk must be a parseable RecorderHeader.
    let content = std::fs::read_to_string(&path).unwrap();
    let first = content
        .lines()
        .next()
        .expect("recording has at least one line");
    let header: RecorderHeader =
        serde_json::from_str(first).expect("first line parses as RecorderHeader");
    assert_eq!(header.schema, RECORDER_SCHEMA);
    assert_eq!(header.vac_version, RECORDER_VAC_VERSION);
    // Commit may be "unknown" in local builds; just ensure it's non-empty.
    assert!(!header.commit.is_empty(), "commit field must be set");
}

#[test]
fn replay_surfaces_recorded_header_and_skips_it() {
    let tmp = TempDir::new().unwrap();
    let cfg = RecorderConfig::new(tmp.path().join("recordings"));
    let mut rec = Recorder::open(cfg).unwrap();
    let events = sample_events();
    for (ev, ts) in &events {
        rec.record_at(ev.clone(), *ts).unwrap();
    }
    let path = rec.current_path().to_path_buf();
    rec.flush().unwrap();
    drop(rec);

    let replay = Replay::open(&path).unwrap();
    let header = replay
        .header()
        .expect("header present for new recordings")
        .clone();
    assert_eq!(header, RecorderHeader::current());
    // Replay iterator must not yield the header as a data line.
    let replayed: Vec<RecordedLine> = replay.collect();
    assert_eq!(replayed.len(), events.len());
}

#[test]
fn replay_without_header_stays_backward_compatible() {
    // Simulate a pre-header recording: write a single data line directly,
    // no header in front of it. Replay must still surface it.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("legacy.jsonl");
    let line = RecordedLine {
        ts_ms: 42,
        event: RecordedInput::Key {
            code: "Enter".into(),
            modifiers: 0,
        },
    };
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&line).unwrap()),
    )
    .unwrap();

    let replay = Replay::open(&path).unwrap();
    assert!(
        replay.header().is_none(),
        "legacy recording must yield no header"
    );
    let replayed: Vec<RecordedLine> = replay.collect();
    assert_eq!(replayed, vec![line]);
}

#[test]
fn replay_warns_on_schema_mismatch_but_still_replays() {
    // Write a header with a bumped schema and a valid data line after it.
    // Replay must expose the recorded (mismatched) header via `header()`
    // and still yield the data line so determinism inspection is possible.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mismatched.jsonl");
    let bogus_header = RecorderHeader {
        schema: "vac-recorder/999".into(),
        vac_version: "0.0.0-from-the-future".into(),
        commit: "deadbeef".into(),
    };
    let line = RecordedLine {
        ts_ms: 7,
        event: RecordedInput::Paste { text: "hi".into() },
    };
    let mut content = String::new();
    content.push_str(&serde_json::to_string(&bogus_header).unwrap());
    content.push('\n');
    content.push_str(&serde_json::to_string(&line).unwrap());
    content.push('\n');
    std::fs::write(&path, content).unwrap();

    let replay = Replay::open(&path).unwrap();
    let observed = replay
        .header()
        .cloned()
        .expect("header present on mismatched file");
    assert_eq!(observed.schema, "vac-recorder/999");
    assert_ne!(observed.schema, RECORDER_SCHEMA);
    let replayed: Vec<RecordedLine> = replay.collect();
    assert_eq!(replayed, vec![line]);
}

#[test]
fn new_variants_survive_json_round_trip() {
    // Guard against accidental serde-tag drift on the variants added
    // in PR-T18 R4.
    for ev in [
        RecordedInput::KeyRelease {
            code: "a".into(),
            modifiers: 0,
        },
        RecordedInput::MouseScroll {
            col: 3,
            row: 4,
            delta_lines: -1,
            delta_cols: 0,
        },
        RecordedInput::Focus { gained: true },
    ] {
        let json = serde_json::to_string(&ev).unwrap();
        let back: RecordedInput = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back, "lossy round-trip for {json}");
    }
}
