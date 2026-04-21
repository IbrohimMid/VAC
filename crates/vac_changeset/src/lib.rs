use std::collections::HashMap;
use std::time::SystemTime;

pub mod formats;

/// Represents the lifecycle state of a file in the changeset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileState {
    /// File was newly created by the agent
    Created,
    /// Existing file was modified
    Modified,
    /// File was removed/deleted
    Removed,
    /// File was successfully reverted to snapshot
    Reverted,
    /// Revert operation failed
    FailedRestore,
}

/// A single entry in the changeset tracking a file's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesetEntry {
    pub path: String,
    pub state: FileState,
    pub source: String,
    pub has_snapshot: bool,
    pub dirty_generation: u32,
    pub last_error: Option<String>,
    pub timestamp: SystemTime,
    pub actor: String,
}

impl ChangesetEntry {
    pub fn new(path: String, state: FileState, actor: String) -> Self {
        Self {
            path,
            state,
            source: String::new(),
            has_snapshot: false,
            dirty_generation: 0,
            last_error: None,
            timestamp: SystemTime::now(),
            actor,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileTreeNode {
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
    pub state: Option<FileState>,
}

#[derive(Debug, Clone, Default)]
pub struct RepoNavigatorState {
    pub file_tree: Vec<FileTreeNode>,
    pub recent_files: Vec<String>,
}

/// Store for tracking all file changes in the current session.
/// Provides a single source of truth for changeset state.
#[derive(Debug, Clone, Default)]
pub struct ChangesetStore {
    entries: Vec<ChangesetEntry>,
    path_index: HashMap<String, usize>,
    generation: u32,
    pub navigator: RepoNavigatorState,
}

impl ChangesetStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn entry_index(&self, path: &str) -> Option<usize> {
        self.path_index.get(path).copied()
    }

    fn push_entry(&mut self, entry: ChangesetEntry) {
        let idx = self.entries.len();
        self.path_index.insert(entry.path.clone(), idx);
        self.entries.push(entry);
    }

    /// Track a newly created file.
    pub fn file_created(&mut self, path: String, actor: String) {
        self.generation += 1;
        if let Some(idx) = self.entry_index(&path) {
            let entry = self.entries.get_mut(idx).expect("path index out of sync");
            entry.state = FileState::Created;
            entry.dirty_generation = self.generation;
            entry.timestamp = SystemTime::now();
            entry.actor = actor;
            entry.last_error = None;
        } else {
            let mut entry = ChangesetEntry::new(path, FileState::Created, actor);
            entry.dirty_generation = self.generation;
            self.push_entry(entry);
        }
    }

    /// Track a file modification. Preserves Created state if file was newly created.
    pub fn file_modified(&mut self, path: String, actor: String, has_snapshot: bool) {
        self.generation += 1;
        if let Some(idx) = self.entry_index(&path) {
            let entry = self.entries.get_mut(idx).expect("path index out of sync");
            if entry.state != FileState::Created {
                entry.state = FileState::Modified;
            }
            entry.dirty_generation = self.generation;
            entry.timestamp = SystemTime::now();
            entry.actor = actor;
            entry.has_snapshot = has_snapshot;
            entry.last_error = None;
        } else {
            let mut entry = ChangesetEntry::new(path, FileState::Modified, actor);
            entry.dirty_generation = self.generation;
            entry.has_snapshot = has_snapshot;
            self.push_entry(entry);
        }
    }

    /// Track a file removal/deletion.
    pub fn file_removed(&mut self, path: String, actor: String) {
        self.generation += 1;
        if let Some(idx) = self.entry_index(&path) {
            let entry = self.entries.get_mut(idx).expect("path index out of sync");
            entry.state = FileState::Removed;
            entry.dirty_generation = self.generation;
            entry.timestamp = SystemTime::now();
            entry.actor = actor;
            entry.last_error = None;
        } else {
            let mut entry = ChangesetEntry::new(path, FileState::Removed, actor);
            entry.dirty_generation = self.generation;
            self.push_entry(entry);
        }
    }

    /// Mark a file as successfully reverted. Creates entry if not exists.
    pub fn revert_success(&mut self, path: &str) {
        self.generation += 1;
        if let Some(idx) = self.entry_index(path) {
            let entry = self.entries.get_mut(idx).expect("path index out of sync");
            entry.state = FileState::Reverted;
            entry.dirty_generation = self.generation;
            entry.timestamp = SystemTime::now();
            entry.last_error = None;
        } else {
            // Create entry if file not tracked yet (edge case: manual revert)
            let mut entry =
                ChangesetEntry::new(path.to_string(), FileState::Reverted, "manual".to_string());
            entry.dirty_generation = self.generation;
            self.push_entry(entry);
        }
    }

    /// Mark a file revert as failed with error message. Creates entry if not exists.
    pub fn revert_failed(&mut self, path: &str, error: String) {
        self.generation += 1;
        if let Some(idx) = self.entry_index(path) {
            let entry = self.entries.get_mut(idx).expect("path index out of sync");
            entry.state = FileState::FailedRestore;
            entry.dirty_generation = self.generation;
            entry.timestamp = SystemTime::now();
            entry.last_error = Some(error);
        } else {
            // Create entry if file not tracked yet (edge case: manual revert)
            let mut entry = ChangesetEntry::new(
                path.to_string(),
                FileState::FailedRestore,
                "manual".to_string(),
            );
            entry.dirty_generation = self.generation;
            entry.last_error = Some(error);
            self.push_entry(entry);
        }
    }

    /// Get all changeset entries.
    pub fn entries(&self) -> &[ChangesetEntry] {
        &self.entries
    }

    /// Active entries: Created, Modified, or Removed (not yet Reverted/FailedRestore).
    pub fn active_entries(&self) -> Vec<&ChangesetEntry> {
        self.entries
            .iter()
            .filter(|e| {
                matches!(
                    e.state,
                    FileState::Created | FileState::Modified | FileState::Removed
                )
            })
            .collect()
    }

    /// Reviewable entries: have a snapshot available for diff/revert.
    pub fn reviewable_entries(&self) -> Vec<&ChangesetEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.has_snapshot && matches!(e.state, FileState::Created | FileState::Modified)
            })
            .collect()
    }

    /// Count entries grouped by state.
    pub fn counts_by_state(&self) -> std::collections::HashMap<FileState, usize> {
        let mut map = std::collections::HashMap::new();
        for e in &self.entries {
            *map.entry(e.state).or_insert(0) += 1;
        }
        map
    }

    /// Get derived view of modified/created files (backward compatible with modified_files Vec).
    pub fn modified_files(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, FileState::Created | FileState::Modified))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Clear all changeset state (used on session restore).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.path_index.clear();
        self.generation = 0;
    }
}

pub fn build_changeset(modified_files: &[String]) -> Vec<ChangesetEntry> {
    modified_files
        .iter()
        .cloned()
        .map(|path| ChangesetEntry::new(path, FileState::Modified, "unknown".to_string()))
        .collect()
}

/// Status of a todo item surfaced from `<todo>` blocks in assistant messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
}

impl TodoStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "[ ]",
            TodoStatus::InProgress => "[/]",
            TodoStatus::Done => "[x]",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoItemType {
    Card,
    ChecklistItem,
    CollapsedIndicator,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub text: String,
    pub status: TodoStatus,
    pub item_type: TodoItemType,
}

impl TodoItem {
    pub fn new(text: String) -> Self {
        Self {
            text,
            status: TodoStatus::Pending,
            item_type: TodoItemType::Card,
        }
    }

    pub fn with_status(mut self, status: TodoStatus) -> Self {
        self.status = status;
        self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_file_created() {
        let mut store = ChangesetStore::new();
        store.file_created("src/main.rs".to_string(), "agent".to_string());

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[0].state, FileState::Created);
        assert_eq!(entries[0].actor, "agent");
    }

    #[test]
    fn test_file_modified() {
        let mut store = ChangesetStore::new();
        store.file_modified("src/lib.rs".to_string(), "agent".to_string(), true);

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/lib.rs");
        assert_eq!(entries[0].state, FileState::Modified);
        assert!(entries[0].has_snapshot);
    }

    #[test]
    fn test_lifecycle_transition_created_to_modified() {
        let mut store = ChangesetStore::new();
        store.file_created("test.rs".to_string(), "agent".to_string());
        store.file_modified("test.rs".to_string(), "agent".to_string(), true);

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, FileState::Created);
    }

    #[test]
    fn test_lifecycle_transition_modified_to_reverted() {
        let mut store = ChangesetStore::new();
        store.file_modified("test.rs".to_string(), "agent".to_string(), true);
        store.revert_success("test.rs");

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, FileState::Reverted);
        assert!(entries[0].last_error.is_none());
    }

    #[test]
    fn test_revert_failed() {
        let mut store = ChangesetStore::new();
        store.file_modified("test.rs".to_string(), "agent".to_string(), true);
        store.revert_failed("test.rs", "Permission denied".to_string());

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, FileState::FailedRestore);
        assert_eq!(entries[0].last_error, Some("Permission denied".to_string()));
    }

    #[test]
    fn test_file_removed() {
        let mut store = ChangesetStore::new();
        store.file_removed("old.rs".to_string(), "agent".to_string());

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, FileState::Removed);
    }

    #[test]
    fn test_modified_files_derived_view() {
        let mut store = ChangesetStore::new();
        store.file_created("a.rs".to_string(), "agent".to_string());
        store.file_modified("b.rs".to_string(), "agent".to_string(), true);
        store.file_removed("c.rs".to_string(), "agent".to_string());
        store.file_modified("d.rs".to_string(), "agent".to_string(), false);
        store.revert_success("d.rs");

        let modified = store.modified_files();
        assert_eq!(modified.len(), 2);
        assert!(modified.contains(&"a.rs".to_string()));
        assert!(modified.contains(&"b.rs".to_string()));
    }

    #[test]
    fn test_generation_increments() {
        let mut store = ChangesetStore::new();
        assert_eq!(store.generation, 0);

        store.file_created("a.rs".to_string(), "agent".to_string());
        assert_eq!(store.generation, 1);

        store.file_modified("b.rs".to_string(), "agent".to_string(), true);
        assert_eq!(store.generation, 2);

        store.revert_success("a.rs");
        assert_eq!(store.generation, 3);
    }

    #[test]
    fn test_clear() {
        let mut store = ChangesetStore::new();
        store.file_created("a.rs".to_string(), "agent".to_string());
        store.file_modified("b.rs".to_string(), "agent".to_string(), true);

        store.clear();
        assert_eq!(store.entries().len(), 0);
        assert_eq!(store.generation, 0);
    }

    #[test]
    fn test_revert_success_creates_entry_if_not_tracked() {
        let mut store = ChangesetStore::new();
        store.revert_success("untracked.rs");

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "untracked.rs");
        assert_eq!(entries[0].state, FileState::Reverted);
        assert_eq!(entries[0].actor, "manual");
    }

    #[test]
    fn test_revert_failed_creates_entry_if_not_tracked() {
        let mut store = ChangesetStore::new();
        store.revert_failed("untracked.rs", "File not found".to_string());

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "untracked.rs");
        assert_eq!(entries[0].state, FileState::FailedRestore);
        assert_eq!(entries[0].last_error, Some("File not found".to_string()));
        assert_eq!(entries[0].actor, "manual");
    }

    #[test]
    fn test_active_entries_excludes_reverted() {
        let mut store = ChangesetStore::new();
        store.file_created("a.rs".to_string(), "agent".to_string());
        store.file_modified("b.rs".to_string(), "agent".to_string(), true);
        store.file_removed("c.rs".to_string(), "agent".to_string());
        store.file_modified("d.rs".to_string(), "agent".to_string(), true);
        store.revert_success("d.rs");

        let active = store.active_entries();
        assert_eq!(active.len(), 3);
        assert!(active.iter().any(|e| e.path == "a.rs"));
        assert!(active.iter().any(|e| e.path == "b.rs"));
        assert!(active.iter().any(|e| e.path == "c.rs"));
        assert!(!active.iter().any(|e| e.path == "d.rs"));
    }

    #[test]
    fn test_reviewable_entries_requires_snapshot() {
        let mut store = ChangesetStore::new();
        store.file_modified("with_snap.rs".to_string(), "agent".to_string(), true);
        store.file_modified("no_snap.rs".to_string(), "agent".to_string(), false);
        store.file_created("created.rs".to_string(), "agent".to_string());

        let reviewable = store.reviewable_entries();
        assert_eq!(reviewable.len(), 1);
        assert_eq!(reviewable[0].path, "with_snap.rs");
    }

    #[test]
    fn test_counts_by_state() {
        let mut store = ChangesetStore::new();
        store.file_created("a.rs".to_string(), "agent".to_string());
        store.file_modified("b.rs".to_string(), "agent".to_string(), true);
        store.file_modified("c.rs".to_string(), "agent".to_string(), true);
        store.revert_success("b.rs");

        let counts = store.counts_by_state();
        assert_eq!(counts.get(&FileState::Created), Some(&1));
        assert_eq!(counts.get(&FileState::Modified), Some(&1));
        assert_eq!(counts.get(&FileState::Reverted), Some(&1));
    }

    #[test]
    fn test_repeated_updates_keep_single_entry() {
        let mut store = ChangesetStore::new();
        store.file_modified("test.rs".to_string(), "agent-a".to_string(), true);
        store.file_removed("test.rs".to_string(), "agent-b".to_string());

        let entries = store.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "test.rs");
        assert_eq!(entries[0].state, FileState::Removed);
        assert_eq!(entries[0].actor, "agent-b");
    }
}
