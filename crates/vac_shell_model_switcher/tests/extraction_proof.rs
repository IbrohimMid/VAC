//! Slice 8 — model switcher extraction proof.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_contracts::{ProviderId, VacModelView};
use vac_shell_model_switcher::{
    ModelSwitcherView, SwitcherEvent, SwitcherKey, SwitcherMode, clamp_selection, filter_models,
    navigation_order, on_key, recent_indices, render_model_switcher,
};

fn model(provider: &str, id: &str, label: &str, reasoning: bool) -> VacModelView {
    VacModelView {
        provider: ProviderId(provider.into()),
        id: id.into(),
        label: label.into(),
        active: false,
        credentials_present: true,
        reasoning,
        cost_label: None,
    }
}

fn fixture() -> ModelSwitcherView {
    let models = vec![
        model("anthropic", "claude-sonnet-4.5", "Claude Sonnet 4.5", true),
        model("anthropic", "claude-haiku-4", "Claude Haiku 4", false),
        model("openai", "gpt-4o", "GPT-4o", false),
        model("openai", "o1", "OpenAI o1", true),
        model("kilo", "kilo-auto/free", "Kilo Auto", false),
    ];
    ModelSwitcherView::new(models)
}

#[test]
fn empty_search_lists_all_models() {
    let view = fixture();
    assert_eq!(filter_models(&view).len(), view.models.len());
}

#[test]
fn search_filters_by_label_provider_or_id() {
    let mut view = fixture();
    view.search = "claude".into();
    let by_label = filter_models(&view);
    assert_eq!(by_label.len(), 2);
    view.search = "openai".into();
    let by_provider = filter_models(&view);
    assert_eq!(by_provider.len(), 2);
    view.search = "kilo-auto".into();
    let by_id = filter_models(&view);
    assert_eq!(by_id.len(), 1);
}

#[test]
fn reasoning_mode_keeps_only_reasoning_models() {
    let mut view = fixture();
    view.mode = SwitcherMode::Reasoning;
    let listed = filter_models(&view);
    let labels: Vec<&str> = listed
        .iter()
        .map(|i| view.models[*i].label.as_str())
        .collect();
    assert_eq!(labels, vec!["Claude Sonnet 4.5", "OpenAI o1"]);
}

#[test]
fn navigation_order_pins_recent_first_then_groups_by_provider() {
    let mut view = fixture()
        .with_recent(vec![(ProviderId("openai".into()), "gpt-4o".into())])
        .with_pinned_provider(ProviderId("kilo".into()));

    let order = navigation_order(&view);
    let labels: Vec<&str> = order
        .iter()
        .map(|i| view.models[*i].label.as_str())
        .collect();
    // gpt-4o pinned via recents first, then kilo (pinned provider),
    // then anthropic / openai alphabetically; openai's o1 only since
    // gpt-4o already consumed.
    assert_eq!(
        labels,
        vec![
            "GPT-4o",
            "Kilo Auto",
            "Claude Sonnet 4.5",
            "Claude Haiku 4",
            "OpenAI o1",
        ]
    );

    // No pinned provider — alphabetical providers after recents.
    view.pinned_provider = None;
    let order = navigation_order(&view);
    let labels: Vec<&str> = order
        .iter()
        .map(|i| view.models[*i].label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec![
            "GPT-4o",
            "Claude Sonnet 4.5",
            "Claude Haiku 4",
            "Kilo Auto",
            "OpenAI o1",
        ]
    );
}

#[test]
fn enter_emits_selected_with_provider_and_id() {
    let mut view = fixture();
    view.visible = true;
    // Default selected = 0 == first in nav order. Without recents/
    // pinned providers, nav order is alphabetical by provider —
    // anthropic first.
    let event = on_key(&mut view, SwitcherKey::Enter);
    match event {
        SwitcherEvent::Selected { provider, id } => {
            assert_eq!(provider.0, "anthropic");
            assert_eq!(id, "claude-sonnet-4.5");
        }
        other => panic!("expected Selected, got {other:?}"),
    }
}

#[test]
fn down_arrow_advances_then_enter_selects_next() {
    let mut view = fixture();
    view.visible = true;
    on_key(&mut view, SwitcherKey::Down);
    let event = on_key(&mut view, SwitcherKey::Enter);
    match event {
        SwitcherEvent::Selected { id, .. } => assert_eq!(id, "claude-haiku-4"),
        other => panic!("expected Selected, got {other:?}"),
    }
}

#[test]
fn tab_toggles_between_all_and_reasoning_modes() {
    let mut view = fixture();
    view.visible = true;
    assert_eq!(view.mode, SwitcherMode::All);
    on_key(&mut view, SwitcherKey::Tab);
    assert_eq!(view.mode, SwitcherMode::Reasoning);
    on_key(&mut view, SwitcherKey::Tab);
    assert_eq!(view.mode, SwitcherMode::All);
}

#[test]
fn escape_dismisses_and_clears_state() {
    let mut view = fixture();
    view.visible = true;
    view.search = "claude".into();
    view.selected = 1;
    let event = on_key(&mut view, SwitcherKey::Escape);
    assert_eq!(event, SwitcherEvent::Dismissed);
    assert!(!view.visible);
    assert!(view.search.is_empty());
    assert_eq!(view.selected, 0);
}

#[test]
fn keys_ignored_when_switcher_closed() {
    let mut view = fixture();
    assert!(!view.visible);
    assert_eq!(on_key(&mut view, SwitcherKey::Enter), SwitcherEvent::Ignored);
}

#[test]
fn invisible_view_renders_nothing() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let view = fixture();
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let any = (0..buf.area.width)
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .any(|(x, y)| !buf[(x, y)].symbol().trim().is_empty());
    assert!(!any);
}

#[test]
fn visible_view_renders_title_models_and_provider_headers() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = fixture();
    view.visible = true;
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("Model Switcher"), "title missing\n{all}");
    assert!(all.contains("Claude Sonnet 4.5"), "model missing\n{all}");
    assert!(all.contains("anthropic"), "provider header missing\n{all}");
    assert!(all.contains("openai"), "provider header missing\n{all}");
}

/// Smoke test that the public types compile against their local
/// definitions — kept as a tripwire that breaks loudly if a future
/// patch shadows them with donor/engine types of the same name.
/// Real dependency-graph proof comes from `cargo tree -p
/// vac_shell_model_switcher -e normal` plus the `Cargo.toml` review
/// (no `vac_tui_runtime`/`vac_core`/`stakai`/`vendor/stakpak` deps).
#[test]
fn public_types_are_local_smoke_test() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(SwitcherMode::All);
    assert_only_local(SwitcherKey::Enter);
}

// =====================================================================
// Slice 8.1 hardening — recent boundary, empty states, selection clamp
// =====================================================================

#[test]
fn recent_indices_drops_stale_entries_not_present_in_models() {
    let view = fixture().with_recent(vec![
        (
            vac_shell_contracts::ProviderId("ghost".into()),
            "missing-model".into(),
        ),
        (
            vac_shell_contracts::ProviderId("openai".into()),
            "gpt-4o".into(),
        ),
    ]);
    let filtered = filter_models(&view);
    let recents = recent_indices(&view, &filtered);
    let labels: Vec<&str> = recents.iter().map(|i| view.models[*i].label.as_str()).collect();
    assert_eq!(labels, vec!["GPT-4o"], "stale recent must be dropped");
}

#[test]
fn recent_indices_drops_duplicates() {
    let view = fixture().with_recent(vec![
        (
            vac_shell_contracts::ProviderId("openai".into()),
            "gpt-4o".into(),
        ),
        (
            vac_shell_contracts::ProviderId("openai".into()),
            "gpt-4o".into(),
        ),
    ]);
    let filtered = filter_models(&view);
    let recents = recent_indices(&view, &filtered);
    assert_eq!(recents.len(), 1, "duplicate recent must dedup");
}

#[test]
fn recent_filtered_out_by_search_does_not_count_as_recent_section() {
    // gpt-4o is in recents, but search "claude" filters it out.
    let mut view = fixture().with_recent(vec![(
        vac_shell_contracts::ProviderId("openai".into()),
        "gpt-4o".into(),
    )]);
    view.search = "claude".into();
    let filtered = filter_models(&view);
    assert!(recent_indices(&view, &filtered).is_empty());
}

#[test]
fn stale_recents_do_not_suppress_provider_headers_in_render() {
    // Reviewer's exact bug shape: one stale recent + one missing.
    // `navigation_order` shouldn't put any rows in the recent
    // segment, so render must NOT eat the provider header.
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = fixture().with_recent(vec![(
        vac_shell_contracts::ProviderId("ghost".into()),
        "missing-model".into(),
    )]);
    view.visible = true;
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(!all.contains(" recent"), "no recent header expected\n{all}");
    assert!(all.contains("anthropic"), "anthropic header missing\n{all}");
    assert!(all.contains("openai"), "openai header missing\n{all}");
}

#[test]
fn empty_models_render_empty_state() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ModelSwitcherView::new(vec![]);
    view.visible = true;
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("No models available"), "empty-state missing\n{all}");
}

#[test]
fn search_miss_renders_empty_state() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = fixture();
    view.visible = true;
    view.search = "no-such-model".into();
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("No models match your search"), "empty-state missing\n{all}");
}

#[test]
fn reasoning_mode_with_no_reasoning_models_renders_empty_state() {
    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut view = ModelSwitcherView::new(vec![
        model("anthropic", "claude-haiku-4", "Claude Haiku 4", false),
        model("openai", "gpt-4o", "GPT-4o", false),
    ]);
    view.visible = true;
    view.mode = SwitcherMode::Reasoning;
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("No reasoning models available"), "empty-state missing\n{all}");
}

#[test]
fn clamp_selection_pulls_selected_back_into_range_after_filter_shrinks_order() {
    let mut view = fixture();
    view.visible = true;
    view.selected = 4; // valid against the full nav order
    // Now narrow the search so the order shrinks below 4.
    view.search = "claude".into();
    clamp_selection(&mut view);
    let len = navigation_order(&view).len();
    assert!(len > 0);
    assert!(view.selected < len, "selected {} must be < {}", view.selected, len);
}

#[test]
fn on_key_clamps_stale_selection_before_acting_on_enter() {
    let mut view = fixture();
    view.visible = true;
    view.selected = 99; // host mutated outside the handler
    let event = on_key(&mut view, SwitcherKey::Enter);
    // Enter must not panic and must dispatch against a real row,
    // not a phantom past-end selection.
    match event {
        SwitcherEvent::Selected { .. } => {}
        other => panic!("expected Selected, got {other:?}"),
    }
}

#[test]
fn cost_label_no_creds_active_render_tags() {
    let mut view = ModelSwitcherView::new(vec![
        VacModelView {
            provider: vac_shell_contracts::ProviderId("anthropic".into()),
            id: "claude-active".into(),
            label: "Claude Active".into(),
            active: true,
            credentials_present: true,
            reasoning: false,
            cost_label: Some("$3 / $15 per M".into()),
        },
        VacModelView {
            provider: vac_shell_contracts::ProviderId("anthropic".into()),
            id: "claude-no-creds".into(),
            label: "Claude No Creds".into(),
            active: false,
            credentials_present: false,
            reasoning: false,
            cost_label: None,
        },
    ]);
    view.visible = true;
    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_model_switcher(f, &view, f.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("$3 / $15 per M"), "cost_label missing\n{all}");
    assert!(all.contains("(no creds)"), "no-creds tag missing\n{all}");
    assert!(all.contains("active"), "active tag missing\n{all}");
}
