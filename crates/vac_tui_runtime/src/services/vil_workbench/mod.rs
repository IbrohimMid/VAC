//! VIL Issue Workstation renderer and grouping logic (Wave 4.1, Unit 9).
//!
//! Consumes `AppState.vil.status.validation_issues` (Vec<VilIssue>) and renders
//! a tabs-within-tab UI with a scrollable issue list on the left and a lineage
//! panel on the right.
//!
//! Quick-actions (`R`/`A`/`D`/`O`) are dispatched via the sibling handler
//! module [`crate::handlers::vil_workbench`].

pub mod render;

use crate::app::{AppState, VilIssue, VilIssueKind};

// Re-export public render API
pub use render::render;

/// Ordered list of kinds surfaced in the top strip.
pub const KIND_ORDER: &[VilIssueKind] = &[
    VilIssueKind::Semantic,
    VilIssueKind::ZeroCopy,
    VilIssueKind::Plumbing,
    VilIssueKind::IrDrift,
    VilIssueKind::CanonicalTerm,
    VilIssueKind::Other,
];

/// Group all issues from state.
pub fn classify_issues(state: &AppState) -> Vec<&VilIssue> {
    state.vil.status.validation_issues.iter().collect()
}

/// Count issues per kind.
pub fn group_counts(issues: &[&VilIssue]) -> std::collections::HashMap<VilIssueKind, usize> {
    let mut m: std::collections::HashMap<VilIssueKind, usize> = std::collections::HashMap::new();
    for issue in issues {
        *m.entry(issue.kind).or_insert(0) += 1;
    }
    m
}

/// Apply the state's active filter to the issue list.
pub fn filtered<'a>(state: &AppState, issues: &'a [&VilIssue]) -> Vec<&'a VilIssue> {
    match state.vil.workbench_group_filter {
        Some(kind) => issues.iter().filter(|i| i.kind == kind).copied().collect(),
        None => issues.to_vec(),
    }
}

/// Build a heatmap: file path → counts per severity.
pub fn heatmap_by_file(
    issues: &[&VilIssue],
) -> std::collections::BTreeMap<String, std::collections::HashMap<crate::app::VilSeverity, usize>> {
    let mut map: std::collections::BTreeMap<String, std::collections::HashMap<crate::app::VilSeverity, usize>> =
        std::collections::BTreeMap::new();
    for issue in issues {
        if let Some(file) = &issue.file {
            *map.entry(file.clone())
                .or_default()
                .entry(issue.severity)
                .or_insert(0) += 1;
        }
    }
    map
}

/// Detect overlapping/conflicting rulebooks: two active books that share a prefix.
pub fn detect_rulebook_conflicts(active_rulebooks: &[String]) -> Vec<(String, String)> {
    let mut conflicts = Vec::new();
    for i in 0..active_rulebooks.len() {
        for j in (i + 1)..active_rulebooks.len() {
            let a = &active_rulebooks[i];
            let b = &active_rulebooks[j];
            let a_base = a.split('/').next().unwrap_or(a);
            let b_base = b.split('/').next().unwrap_or(b);
            if a_base == b_base {
                conflicts.push((a.clone(), b.clone()));
            }
        }
    }
    conflicts
}

/// Return the currently-selected issue, if any, honoring the active filter.
pub fn selected_issue(state: &AppState) -> Option<VilIssue> {
    let issues = classify_issues(state);
    let view = filtered(state, &issues);
    view.get(state.vil.workbench_selected).map(|i| (*i).clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::{AppStateOptions, VilIssue};

    fn fresh_state(issues: Vec<String>) -> AppState {
        let mut s = AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap_or_default(),
        });
        s.vil.status.validation_issues = issues.into_iter().map(VilIssue::from_raw).collect();
        s
    }

    #[test]
    fn classify_keyword_matrix() {
        assert_eq!(
            VilIssueKind::classify("Handler 'a' param 'b' contains owned-bytes type 'Vec<u8>'"),
            VilIssueKind::ZeroCopy
        );
        assert_eq!(
            VilIssueKind::classify(
                "Struct 'X' manually implements 'VilMessage' (medium — trait impl only)."
            ),
            VilIssueKind::Plumbing
        );
        assert_eq!(
            VilIssueKind::classify(
                "Struct 'Foo' looks like a semantic message type but has no VIL role macro. Add #[vil_state]"
            ),
            VilIssueKind::Semantic
        );
        assert_eq!(
            VilIssueKind::classify("IR metadata drift detected on src/lib.rs"),
            VilIssueKind::IrDrift
        );
        assert_eq!(
            VilIssueKind::classify("Canonical term violation: use 'changeset' not 'diff-set'"),
            VilIssueKind::CanonicalTerm
        );
        assert_eq!(
            VilIssueKind::classify("Something completely unrelated."),
            VilIssueKind::Other
        );
    }

    #[test]
    fn grouping_counts_five_mock_issues() {
        let state = fresh_state(vec![
            "Handler 'a' zero-copy violation on Network boundary".into(),
            "Struct 'B' manually implements 'VilMessage' — remove plumbing".into(),
            "Struct 'C' has no VIL role macro — add #[vil_state]".into(),
            "IR drift detected between HEAD and working tree".into(),
            "Some other advisory from a future pass.".into(),
        ]);

        let issues = classify_issues(&state);
        assert_eq!(issues.len(), 5);
        let counts = group_counts(&issues);
        assert_eq!(counts.get(&VilIssueKind::ZeroCopy).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Plumbing).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Semantic).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::IrDrift).copied().unwrap_or(0), 1);
        assert_eq!(counts.get(&VilIssueKind::Other).copied().unwrap_or(0), 1);
    }

    #[test]
    fn filtered_respects_group_filter() {
        let mut state = fresh_state(vec![
            "Handler 'a' zero-copy violation".into(),
            "Struct 'B' manually implements 'VilMessage'".into(),
            "Handler 'c' owned-bytes type 'Vec<u8>'".into(),
        ]);
        state.vil.workbench_group_filter = None;
        {
            let all = classify_issues(&state);
            assert_eq!(filtered(&state, &all).len(), 3);
        }
        state.vil.workbench_group_filter = Some(VilIssueKind::ZeroCopy);
        {
            let all = classify_issues(&state);
            let view = filtered(&state, &all);
            assert_eq!(view.len(), 2);
            assert!(view.iter().all(|i| i.kind == VilIssueKind::ZeroCopy));
        }
    }

    #[test]
    fn vil_issue_from_raw_extracts_file_and_kind() {
        let raw = "Handler 'create_user' is on Network boundary but is not zero-copy eligible.";
        let issue = VilIssue::from_raw(raw.to_string());
        assert_eq!(issue.file.as_deref(), Some("create_user"));
        assert_eq!(issue.kind, VilIssueKind::ZeroCopy);
    }

    #[test]
    fn short_collapses_whitespace_and_truncates() {
        let long = "a ".repeat(200);
        let short = crate::app::runtime::shorten(&long);
        assert!(short.len() <= 160);
    }

    #[test]
    fn heatmap_aggregates_per_file() {
        let issues: Vec<VilIssue> = vec![
            VilIssue::from_raw("Handler 'create_user' zero-copy violation".to_string()),
            VilIssue::from_raw("Handler 'create_user' owned-bytes type Vec<u8>".to_string()),
            VilIssue::from_raw("Struct 'Foo' no VIL role macro in delete_user".to_string()),
        ];
        let refs: Vec<&VilIssue> = issues.iter().collect();
        let map = heatmap_by_file(&refs);
        let create_user_count: usize = map
            .get("create_user")
            .map(|m| m.values().sum())
            .unwrap_or(0);
        assert_eq!(create_user_count, 2);
    }

    #[test]
    fn conflict_detector_flags_overlapping_rules() {
        let books = vec![
            "security/auth".to_string(),
            "security/crypto".to_string(),
            "performance/alloc".to_string(),
        ];
        let conflicts = detect_rulebook_conflicts(&books);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].0.starts_with("security/") || conflicts[0].1.starts_with("security/"));
    }

    #[test]
    fn repair_preview_uses_typed_proposal() {
        let issue = VilIssue::from_raw(
            "Handler 'send' param 'buf' contains owned-bytes type 'Vec<u8>'".to_string(),
        );
        assert!(issue.repair_proposal.is_some());
        assert!(
            issue
                .repair_proposal
                .as_deref()
                .unwrap()
                .contains("ShmSlice")
        );
    }

    #[test]
    fn grouped_issue_rendering_by_kind() {
        let state = fresh_state(vec![
            "Handler 'a' owned-bytes type Vec<u8>".into(),
            "Struct 'B' manually implements VilMessage".into(),
            "Handler 'c' owned-bytes type Vec<u8>".into(),
        ]);
        let issues = classify_issues(&state);
        let counts = group_counts(&issues);
        assert_eq!(counts.get(&VilIssueKind::ZeroCopy).copied().unwrap_or(0), 2);
        assert_eq!(counts.get(&VilIssueKind::Plumbing).copied().unwrap_or(0), 1);
    }
}
