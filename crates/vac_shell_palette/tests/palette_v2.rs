//! Slice 18 — palette v2 enrichment proofs.

use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec};
use vac_shell_palette::{fuzzy_filter, group_by_category, rank_entries};

fn spec(id: &str, slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: id.into(),
        slash: slash.into(),
        title: id.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        shortcut: None,
        ..Default::default()
    }
}

#[test]
fn fuzzy_search_by_alias() {
    let mut s = spec("model", "/model");
    s.aliases = vec!["m".into(), "switch-model".into()];
    let listed = fuzzy_filter("switch", &[s.clone()]);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].slash, "/model");
}

#[test]
fn fuzzy_search_by_keyword() {
    let mut s = spec("review", "/review");
    s.keywords = vec!["diff".into(), "patch".into()];
    let listed = fuzzy_filter("patch", &[s]);
    assert_eq!(listed.len(), 1);
}

#[test]
fn fuzzy_filter_empty_input_returns_visible_only() {
    let mut a = spec("model", "/model");
    a.palette_visible = true;
    let mut b = spec("hidden", "/hidden");
    b.palette_visible = false;
    let listed = fuzzy_filter("", &[a, b]);
    assert_eq!(listed.len(), 1);
}

#[test]
fn disabled_command_still_lists_with_reason() {
    let mut a = spec("offline", "/offline");
    a.disabled_reason = Some("network unreachable".into());
    let listed = fuzzy_filter("offline", &[a.clone()]);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].disabled_reason.as_deref(),
        Some("network unreachable")
    );
}

#[test]
fn rank_entries_recents_first_then_enabled_then_disabled() {
    let mut a = spec("alpha", "/alpha");
    let mut b = spec("beta", "/beta");
    let mut c = spec("gamma", "/gamma");
    let mut d = spec("delta", "/delta");
    d.disabled_reason = Some("foo".into());
    a.aliases = vec![];
    b.aliases = vec![];
    c.aliases = vec![];
    let ranked = rank_entries(&[a, b, c, d], &["beta".into(), "gamma".into()]);
    let ids: Vec<&str> = ranked.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["beta", "gamma", "alpha", "delta"]);
}

#[test]
fn rank_entries_dedups_duplicate_recent_ids() {
    let a = spec("alpha", "/alpha");
    let b = spec("beta", "/beta");
    let ranked = rank_entries(
        &[a, b],
        &[
            "alpha".into(),
            "alpha".into(),
            "alpha".into(),
            "beta".into(),
        ],
    );
    let ids: Vec<&str> = ranked.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha", "beta"]);
}

#[test]
fn disabled_recent_stays_in_recent_block_and_keeps_reason() {
    let a = spec("alpha", "/alpha");
    let mut b = spec("beta", "/beta");
    b.disabled_reason = Some("offline".into());
    let ranked = rank_entries(&[a, b], &["beta".into()]);
    // beta is disabled but pinned in recents → comes first.
    assert_eq!(ranked[0].id, "beta");
    assert_eq!(ranked[0].disabled_reason.as_deref(), Some("offline"));
    // alpha then follows.
    assert_eq!(ranked[1].id, "alpha");
}

#[test]
fn group_by_category_buckets_entries_with_none_for_untyped() {
    let mut a = spec("model", "/model");
    a.category = Some("Models".into());
    let mut b = spec("review", "/review");
    b.category = Some("Code".into());
    let c = spec("misc", "/misc");
    let groups = group_by_category(&[a, b, c]);
    let cats: Vec<Option<String>> = groups.iter().map(|(c, _)| c.clone()).collect();
    assert!(cats.contains(&None));
    assert!(cats.contains(&Some("Models".into())));
    assert!(cats.contains(&Some("Code".into())));
}
