#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesetEntry {
    pub path: String,
}

pub fn build_changeset(modified_files: &[String]) -> Vec<ChangesetEntry> {
    modified_files
        .iter()
        .cloned()
        .map(|path| ChangesetEntry { path })
        .collect()
}

