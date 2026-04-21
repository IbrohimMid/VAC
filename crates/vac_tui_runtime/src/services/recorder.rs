//! Input recorder + replay (PR-T18).
//!
//! Writes a JSON-lines stream of user input events (keys, mouse, resize,
//! paste) to `.vac/recordings/<timestamp>.jsonl` so sessions can be
//! deterministically re-played via `vac tui --replay <file>`.
//!
//! The recorder intentionally does *not* serialize the full [`InputEvent`]
//! union — most variants carry backend state that has no meaning on
//! replay. We record only inputs the user produced at the terminal, which
//! is both sufficient for deterministic replay (the rest of the state is a
//! pure function of those inputs) and avoids a combinatorial serde burden.
//!
//! Rotation policy:
//!   - New file rolled when the current file exceeds [`DEFAULT_MAX_BYTES`]
//!     (10 MB). One file per rollover; filenames are monotonic.
//!   - Keep at most [`DEFAULT_MAX_FILES`] (20) files in the directory,
//!     oldest pruned first. Sort order is by filename, which is safe
//!     because we mint timestamped names.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_MAX_FILES: usize = 20;

/// User-facing input events that are safe + useful to replay. Mirrors the
/// subset of crossterm-derived [`InputEvent`] variants consumed by the
/// global input pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RecordedInput {
    Key { code: String, modifiers: u8 },
    MouseDragStart { col: u16, row: u16 },
    MouseDrag { col: u16, row: u16 },
    MouseDragEnd { col: u16, row: u16 },
    Resize { cols: u16, rows: u16 },
    Paste { text: String },
}

/// One recorded line: a wall-clock timestamp plus the event payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedLine {
    /// Milliseconds since Unix epoch at the time of recording.
    pub ts_ms: u128,
    pub event: RecordedInput,
}

// ── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RecorderConfig {
    pub dir: PathBuf,
    pub max_bytes_per_file: u64,
    pub max_files: usize,
}

impl RecorderConfig {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            max_bytes_per_file: DEFAULT_MAX_BYTES,
            max_files: DEFAULT_MAX_FILES,
        }
    }

    pub fn with_max_bytes(mut self, bytes: u64) -> Self {
        self.max_bytes_per_file = bytes;
        self
    }

    pub fn with_max_files(mut self, files: usize) -> Self {
        self.max_files = files;
        self
    }
}

// ── Recorder ─────────────────────────────────────────────────────────────────

pub struct Recorder {
    cfg: RecorderConfig,
    current_path: PathBuf,
    file: File,
    bytes_written: u64,
}

impl Recorder {
    pub fn open(cfg: RecorderConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(&cfg.dir)?;
        let path = mint_path(&cfg.dir, 0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            cfg,
            current_path: path,
            file,
            bytes_written: 0,
        })
    }

    pub fn current_path(&self) -> &Path {
        &self.current_path
    }

    /// Append one event. Rotates (and prunes) when the current file
    /// crosses the size limit.
    pub fn record(&mut self, event: RecordedInput) -> std::io::Result<()> {
        self.record_at(event, now_ms())
    }

    /// Test hook: record with a caller-supplied timestamp for reproducible
    /// round-trip assertions.
    pub fn record_at(&mut self, event: RecordedInput, ts_ms: u128) -> std::io::Result<()> {
        let line = RecordedLine { ts_ms, event };
        let json = serde_json::to_string(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let bytes = json.len() as u64 + 1; // trailing newline
        if self.bytes_written + bytes > self.cfg.max_bytes_per_file && self.bytes_written > 0 {
            self.rotate()?;
        }
        self.file.write_all(json.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.bytes_written += bytes;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.file.flush()?;
        // Allocate a new monotonic filename. Mint a unique suffix by
        // scanning the directory for the highest existing ordinal so
        // two rapid rotations in the same millisecond don't collide.
        let next = next_ordinal(&self.cfg.dir)?;
        let new_path = mint_path(&self.cfg.dir, next);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)?;
        self.current_path = new_path;
        self.bytes_written = 0;
        prune_old_files(&self.cfg.dir, self.cfg.max_files)?;
        Ok(())
    }
}

fn mint_path(dir: &Path, ordinal: u32) -> PathBuf {
    let ts = now_ms();
    dir.join(format!("{ts:013}-{ordinal:04}.jsonl"))
}

fn next_ordinal(dir: &Path) -> std::io::Result<u32> {
    let mut max: i64 = -1;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                // Format: <ts>-<ord>.jsonl
                if let Some(stem) = name.strip_suffix(".jsonl") {
                    if let Some((_, ord)) = stem.rsplit_once('-') {
                        if let Ok(n) = ord.parse::<u32>() {
                            max = max.max(n as i64);
                        }
                    }
                }
            }
        }
    }
    Ok((max + 1) as u32)
}

fn prune_old_files(dir: &Path, max_files: usize) -> std::io::Result<()> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect(),
        Err(_) => return Ok(()),
    };
    if files.len() <= max_files {
        return Ok(());
    }
    // Filename prefix embeds the creation timestamp, so a lexicographic
    // sort gives oldest-first.
    files.sort();
    let drop_count = files.len() - max_files;
    for path in files.into_iter().take(drop_count) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ── Replay reader ────────────────────────────────────────────────────────────

/// Iterator-style reader over a single recorded file. Corrupt lines are
/// skipped — a torn line at the end of a crashed session must not stop
/// replay of everything that came before.
pub struct Replay {
    lines: std::io::Lines<BufReader<File>>,
}

impl Replay {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            lines: BufReader::new(file).lines(),
        })
    }
}

impl Iterator for Replay {
    type Item = RecordedLine;

    fn next(&mut self) -> Option<Self::Item> {
        for line in self.lines.by_ref() {
            let line = match line {
                Ok(l) if !l.trim().is_empty() => l,
                Ok(_) => continue,
                Err(_) => return None,
            };
            if let Ok(parsed) = serde_json::from_str::<RecordedLine>(&line) {
                return Some(parsed);
            }
            // Malformed line — skip and keep going.
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_events() -> Vec<(RecordedInput, u128)> {
        vec![
            (RecordedInput::Key { code: "Enter".into(), modifiers: 0 }, 100),
            (RecordedInput::Key { code: "a".into(), modifiers: 0 }, 200),
            (RecordedInput::MouseDragStart { col: 5, row: 10 }, 300),
            (RecordedInput::Paste { text: "hello world".into() }, 400),
            (RecordedInput::Resize { cols: 120, rows: 40 }, 500),
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
            .filter(|e| e.file_name().to_str().map(|n| n.ends_with(".jsonl")).unwrap_or(false))
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
            event: RecordedInput::Key { code: "X".into(), modifiers: 0 },
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
}
