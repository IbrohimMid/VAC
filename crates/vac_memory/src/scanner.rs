//! Walk `<root>/.vac/memory/{active,archived,team}/` and materialize
//! every file as [`Memory`]. Filesystem errors on individual files are
//! logged and skipped — a single malformed file must not poison a
//! consolidator run or retrieval call.

use std::path::{Path, PathBuf};

use tokio::fs;
use tracing::warn;

use crate::error::MemoryResult;
use crate::memdir::{self, Memory, MemoryKind};

/// Walks a memdir and yields parsed [`Memory`] values.
#[derive(Debug, Clone)]
pub struct MemoryScanner {
    root: PathBuf,
}

impl MemoryScanner {
    /// `<project>/.vac/memory/` — the three shelf dirs are nested
    /// under this root. Created on demand by `ensure_layout`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create `active/`, `archived/`, `team/` if missing. Idempotent.
    pub async fn ensure_layout(&self) -> MemoryResult<()> {
        for kind in MemoryKind::all() {
            fs::create_dir_all(self.root.join(kind.dir_name())).await?;
        }
        Ok(())
    }

    /// Scan every shelf. Malformed files are logged + skipped, not
    /// propagated as errors.
    pub async fn scan_all(&self) -> MemoryResult<Vec<Memory>> {
        let mut out = Vec::new();
        for kind in MemoryKind::all() {
            out.extend(self.scan_kind(kind).await?);
        }
        Ok(out)
    }

    /// Scan one shelf. See [`Self::scan_all`] for error semantics.
    pub async fn scan_kind(&self, kind: MemoryKind) -> MemoryResult<Vec<Memory>> {
        let dir = self.root.join(kind.dir_name());
        let mut out = Vec::new();
        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match self.load_one(kind, &path).await {
                Ok(m) => out.push(m),
                Err(e) => warn!(
                    target: "vac_memory",
                    path = %path.display(),
                    error = %e,
                    "skipping malformed memory file",
                ),
            }
        }
        Ok(out)
    }

    async fn load_one(&self, kind: MemoryKind, path: &Path) -> MemoryResult<Memory> {
        let raw = fs::read_to_string(path).await?;
        let (frontmatter, body) = memdir::parse(&raw)?;
        Ok(Memory {
            kind,
            path: path.to_path_buf(),
            frontmatter,
            body,
        })
    }

    /// Append (or overwrite) a memory file. Uses a temp + rename to
    /// keep partial writes off-disk. Refreshes `updated_at` unless the
    /// caller has already set it.
    pub async fn write(&self, mut mem: Memory) -> MemoryResult<Memory> {
        self.ensure_layout().await?;
        if mem.frontmatter.updated_at.is_none() {
            mem.frontmatter.updated_at = Some(chrono::Utc::now());
        }
        let path = memdir::path_for(&self.root, mem.kind, &mem.frontmatter.topic);
        let contents = memdir::serialize(&mem.frontmatter, &mem.body)?;
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, contents.as_bytes()).await?;
        fs::rename(&tmp, &path).await?;
        mem.path = path;
        Ok(mem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memdir::MemoryFrontmatter;

    fn fm(topic: &str) -> MemoryFrontmatter {
        MemoryFrontmatter {
            topic: topic.into(),
            title: None,
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: None,
            importance: 0.5,
            source_policy: None,
        }
    }

    #[tokio::test]
    async fn scan_empty_root_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let s = MemoryScanner::new(tmp.path().to_path_buf());
        let all = s.scan_all().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn write_then_scan_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let s = MemoryScanner::new(tmp.path().to_path_buf());
        let m = Memory {
            kind: MemoryKind::Active,
            path: PathBuf::new(),
            frontmatter: fm("build"),
            body: "nextest please\n".into(),
        };
        s.write(m).await.unwrap();
        let all = s.scan_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].frontmatter.topic, "build");
        assert!(all[0].body.contains("nextest"));
    }

    #[tokio::test]
    async fn malformed_file_is_skipped_not_propagated() {
        let tmp = tempfile::tempdir().unwrap();
        let s = MemoryScanner::new(tmp.path().to_path_buf());
        s.ensure_layout().await.unwrap();
        // Write a valid + malformed file on the same shelf.
        let good = Memory {
            kind: MemoryKind::Active,
            path: PathBuf::new(),
            frontmatter: fm("good"),
            body: "ok".into(),
        };
        s.write(good).await.unwrap();
        fs::write(
            tmp.path().join("active").join("bad.md"),
            b"not frontmatter at all",
        )
        .await
        .unwrap();
        let all = s.scan_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].frontmatter.topic, "good");
    }
}
