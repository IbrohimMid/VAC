//! Slice 8.2 — host projection acceptance proof.
//!
//! Builds an `InMemoryModelSource`, projects it to a
//! `ModelSwitcherView` via `build_switcher_view`, drives the widget
//! a couple of keystrokes, and asserts the emitted `Selected` intent
//! matches the host's source. The widget never mutates the source —
//! the host applies the model switch in slice 9 (not here).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_contracts::ProviderId;
use vac_shell_host_model::{InMemoryModelSource, build_switcher_view, project_models};
use vac_shell_model_switcher::{
    SwitcherEvent, SwitcherKey, navigation_order, on_key, render_model_switcher,
};

fn source() -> InMemoryModelSource {
    InMemoryModelSource::new()
        .with_provider("anthropic", true)
        .with_provider("openai", true)
        .with_provider("kilo", false)
        .with_model(
            "anthropic",
            "claude-sonnet-4.5",
            "Claude Sonnet 4.5",
            true,
            Some("$3 / $15 per M"),
        )
        .with_model("anthropic", "claude-haiku-4", "Claude Haiku 4", false, None)
        .with_model("openai", "gpt-4o", "GPT-4o", false, None)
        .with_model("kilo", "kilo-auto", "Kilo Auto", false, None)
        .with_active("anthropic", "claude-sonnet-4.5")
        .with_recent("openai", "gpt-4o")
}

#[test]
fn projected_view_contains_all_models_with_active_marked_once() {
    let src = source();
    let models = project_models(&src);
    assert_eq!(models.len(), 4);
    let actives: Vec<&str> = models
        .iter()
        .filter(|m| m.active)
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(actives, vec!["claude-sonnet-4.5"]);
}

#[test]
fn navigation_order_starts_with_recent_then_groups() {
    let src = source();
    let view = build_switcher_view(&src, 5);
    let order = navigation_order(&view);
    let labels: Vec<&str> = order
        .iter()
        .map(|i| view.models[*i].label.as_str())
        .collect();
    // openai/gpt-4o is the only recent entry. After it, providers
    // sort alphabetically (no pinned set on this source).
    assert_eq!(
        labels,
        vec!["GPT-4o", "Claude Sonnet 4.5", "Claude Haiku 4", "Kilo Auto",]
    );
}

#[test]
fn enter_emits_intent_for_the_selected_row_without_mutating_source() {
    let src = source();
    let mut view = build_switcher_view(&src, 5);
    view.visible = true;

    // Default selection is 0 → first nav row → recent gpt-4o.
    let event = on_key(&mut view, SwitcherKey::Enter);
    match event {
        SwitcherEvent::Selected { provider, id } => {
            assert_eq!(provider, ProviderId("openai".into()));
            assert_eq!(id, "gpt-4o");
        }
        other => panic!("expected Selected, got {other:?}"),
    }
    // Source untouched — host applies the switch later, not here.
    assert_eq!(
        src.active,
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into())),
    );
}

#[test]
fn pinned_provider_overrides_alphabetical_after_recents() {
    let src = source().with_pinned("kilo");
    let view = build_switcher_view(&src, 5);
    let order = navigation_order(&view);
    let labels: Vec<&str> = order
        .iter()
        .map(|i| view.models[*i].label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["GPT-4o", "Kilo Auto", "Claude Sonnet 4.5", "Claude Haiku 4",]
    );
}

/// Slice 8.2a — full chain proof: source → projection → widget
/// render. Builds an `InMemoryModelSource`, projects via
/// `build_switcher_view`, draws into a ratatui `TestBackend`, and
/// asserts the buffer contains every UI artefact that depends on a
/// real source: the title, a recent row label, an active-tag, a
/// no-creds tag, a cost label, and a provider header. This is the
/// piece slice 8.2's first acceptance proof was missing.
#[test]
fn source_projection_renders_model_switcher_widget() {
    let src = source();
    let mut view = build_switcher_view(&src, 5);
    view.visible = true;

    let backend = TestBackend::new(100, 24);
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

    assert!(all.contains("Model Switcher"), "title missing\n{all}");
    assert!(all.contains("GPT-4o"), "recent label missing\n{all}");
    assert!(
        all.contains("Claude Sonnet 4.5"),
        "active model label missing\n{all}"
    );
    assert!(all.contains("active"), "active tag missing\n{all}");
    assert!(
        all.contains("(no creds)"),
        "no-creds tag missing for kilo\n{all}"
    );
    assert!(all.contains("$3 / $15 per M"), "cost_label missing\n{all}");
    assert!(
        all.contains("anthropic"),
        "anthropic provider header missing\n{all}"
    );
}

#[test]
fn missing_provider_credentials_reach_the_view() {
    let src = source();
    let view = build_switcher_view(&src, 5);
    let kilo = view
        .models
        .iter()
        .find(|m| m.id == "kilo-auto")
        .expect("kilo present");
    assert!(!kilo.credentials_present);
}
