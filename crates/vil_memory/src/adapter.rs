//! M7.3 — `VacMemoryBridge` adapter.
//!
//! Bridges the `vil_memory` working-memory/episodic-memory contract
//! onto the filesystem-backed `vac_memory` memdir + retrieval stack
//! (`vac_memory::MemoryScanner` + `vac_memory::query::find_relevant`).
//! Consumers of `vil_memory` (today: `vac_core::engine::VacEngine`,
//! tomorrow: `vil_swarm::planner`) can switch to this bridge without
//! changing the surface they call.
//!
//! Why a bridge first:
//!
//! - The blueprint (`docs/ultraplan-vac-product.md` §6 Risk 2) calls
//!   for atomic swap. The bridge keeps the old trait surface while
//!   delegating to the new store. One commit flips the producer;
//!   consumers don't see a diff.
//! - The old redb path stays usable until every consumer migrates.
//!
//! Scope of this adapter:
//!
//! - Read path: `recall(prompt, k)` → `vac_memory::find_relevant`.
//! - Write path: `append(content, kind)` → `vac_memory::MemoryScanner::write`
//!   as an entry under `active/<topic>.md` with a deterministic
//!   topic derived from the `MemoryType`.
//! - Not ported: the `access_count` field on `MemoryEntry` (redb
//!   tracks it; memdir does not). Callers that need it keep the
//!   redb path.

use std::path::PathBuf;

use tracing::warn;

use crate::error::{MemoryError, MemoryResult};
use crate::store::{MemoryEntry, MemoryType};

/// Bridge from `vil_memory` → `vac_memory`.
///
/// Construct via [`VacMemoryBridge::new`] with the project root.
/// The bridge owns a `MemoryScanner` bound to
/// `<root>/.vac/memory/` and lazily ensures the memdir layout on
/// first write.
pub struct VacMemoryBridge {
    scanner: vac_memory::MemoryScanner,
}

impl VacMemoryBridge {
    /// Construct with a project root. Memdir lives at
    /// `<root>/.vac/memory/{active,archived,team}`.
    pub fn new(project_root: PathBuf) -> Self {
        let root = project_root.join(".vac").join("memory");
        Self {
            scanner: vac_memory::MemoryScanner::new(root),
        }
    }

    /// Recall top-`k` memories relevant to `query`. Results are
    /// mapped onto `vil_memory::MemoryEntry` so the caller's type
    /// signature is unchanged. `MemoryType` is classified by shelf:
    /// `active` → Working, `archived` → Episodic, `team` → Semantic.
    pub async fn recall(
        &self,
        query: &str,
        k: usize,
    ) -> MemoryResult<Vec<MemoryEntry>> {
        let memories = self.scanner.scan_all().await.map_err(to_vil)?;
        let hits =
            vac_memory::find_relevant(&memories, query, k, /* include_archived */ true);
        Ok(hits
            .into_iter()
            .map(|(m, score)| MemoryEntry {
                id: m.frontmatter.topic.clone(),
                content: m.body.clone(),
                memory_type: kind_to_memory_type(m.kind),
                timestamp: m.frontmatter.updated_at.unwrap_or(m.frontmatter.created_at),
                importance: (score.blended as f32).clamp(0.0, 1.0),
                access_count: 0,
            })
            .collect())
    }

    /// Append new content under a deterministic topic. Active shelf
    /// for `Working`/`Semantic`; Archived for `Episodic`. The topic
    /// slug is `working-<yyyymmdd>`, `episodic-<yyyymmdd>`, or
    /// `semantic-<yyyymmdd>` so repeat calls on the same day merge
    /// into one file (scanner's `write` overwrites same-topic).
    pub async fn append(
        &self,
        content: &str,
        kind: MemoryType,
    ) -> MemoryResult<MemoryEntry> {
        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let (shelf, prefix) = match kind {
            MemoryType::Working => (vac_memory::MemoryKind::Active, "working"),
            MemoryType::Episodic => (vac_memory::MemoryKind::Archived, "episodic"),
            MemoryType::Semantic => (vac_memory::MemoryKind::Team, "semantic"),
        };
        let topic = format!("{prefix}-{today}");
        let now = chrono::Utc::now();
        let fm = vac_memory::MemoryFrontmatter {
            topic: topic.clone(),
            title: Some(format!("{prefix} memory entry {today}")),
            tags: vec![prefix.into()],
            created_at: now,
            updated_at: Some(now),
            importance: 0.5,
            source_policy: Some("vac_memory_bridge".into()),
        };
        let mem = vac_memory::Memory {
            kind: shelf,
            path: PathBuf::new(),
            frontmatter: fm,
            body: content.to_string(),
        };
        let written = self.scanner.write(mem).await.map_err(to_vil)?;
        Ok(MemoryEntry {
            id: written.frontmatter.topic,
            content: written.body,
            memory_type: kind,
            timestamp: now,
            importance: 0.5,
            access_count: 0,
        })
    }

    /// Capacity hint — how many entries are on the `Working` shelf
    /// right now. Used by bridges that previously read
    /// `MemoryConfig.working_capacity`.
    pub async fn working_count(&self) -> MemoryResult<usize> {
        let mems = self
            .scanner
            .scan_kind(vac_memory::MemoryKind::Active)
            .await
            .map_err(to_vil)?;
        Ok(mems.len())
    }
}

fn kind_to_memory_type(k: vac_memory::MemoryKind) -> MemoryType {
    match k {
        vac_memory::MemoryKind::Active => MemoryType::Working,
        vac_memory::MemoryKind::Archived => MemoryType::Episodic,
        vac_memory::MemoryKind::Team => MemoryType::Semantic,
        // `MemoryKind` is `#[non_exhaustive]`; a future shelf
        // variant defaults to the generic Working bucket rather
        // than hard-erroring. Re-audit when a new variant lands.
        _ => MemoryType::Working,
    }
}

fn to_vil(e: vac_memory::MemoryError) -> MemoryError {
    warn!(
        target: "vil_memory::adapter",
        error = %e,
        "vac_memory error surfaced through bridge"
    );
    MemoryError::Other(anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recall_returns_relevant_hits() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = VacMemoryBridge::new(tmp.path().to_path_buf());

        // Seed two entries.
        bridge
            .append("cargo nextest is mandatory", MemoryType::Working)
            .await
            .unwrap();
        bridge
            .append("review threads auto-reap after 30 days", MemoryType::Episodic)
            .await
            .unwrap();

        let hits = bridge.recall("nextest", 5).await.unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("nextest"));
    }

    #[tokio::test]
    async fn append_increments_working_count() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = VacMemoryBridge::new(tmp.path().to_path_buf());
        assert_eq!(bridge.working_count().await.unwrap(), 0);
        bridge
            .append("entry", MemoryType::Working)
            .await
            .unwrap();
        assert_eq!(bridge.working_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn append_routes_kind_to_shelf() {
        let tmp = tempfile::tempdir().unwrap();
        let bridge = VacMemoryBridge::new(tmp.path().to_path_buf());
        bridge.append("w", MemoryType::Working).await.unwrap();
        bridge.append("e", MemoryType::Episodic).await.unwrap();
        bridge.append("s", MemoryType::Semantic).await.unwrap();

        let active = tmp.path().join(".vac/memory/active");
        let archived = tmp.path().join(".vac/memory/archived");
        let team = tmp.path().join(".vac/memory/team");
        for (dir, label) in [(active, "active"), (archived, "archived"), (team, "team")] {
            let count = std::fs::read_dir(&dir).unwrap().count();
            assert!(count >= 1, "shelf {label} should have ≥ 1 entry");
        }
    }
}
