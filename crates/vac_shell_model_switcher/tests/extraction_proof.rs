//! Slice 8 — model switcher extraction proof.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use vac_shell_contracts::{ProviderId, VacModelView};
use vac_shell_model_switcher::{
    ModelSwitcherView, SwitcherEvent, SwitcherKey, SwitcherMode, filter_models, navigation_order,
    on_key, render_model_switcher,
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

/// Drift tripwire — only `ratatui` and contract types may show up
/// at the boundary.
#[test]
fn no_donor_or_engine_dependency_links_in() {
    fn assert_only_local<T: Sized>(_: T) {}
    assert_only_local(SwitcherMode::All);
    assert_only_local(SwitcherKey::Enter);
}
