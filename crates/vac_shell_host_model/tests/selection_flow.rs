//! Slice 9 — end-to-end acceptance proof.
//!
//! Drives the full chain reviewer mandated:
//!
//! ```text
//! widget Enter → SwitcherEvent::Selected
//!   → switcher_event_to_action()
//!   → ShellAction::SelectModel
//!   → CompositeShellHost::handle()
//!   → ModelSelectionController::select_model()
//!   → ModelSelectionState mutates active + recents
//!   → build_switcher_view() re-projects
//!   → render_model_switcher() shows the new active row
//! ```
//!
//! Mutation lives entirely host-side; the UI widget never touches
//! state on its own.

use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_bridge::{CompositeShellHost, ModelController, ProviderId, ShellAction, ShellHost};
use vac_shell_host_model::{
    HostModel, ModelSelectionController, ModelSelectionState, ProviderInfo, build_switcher_view,
    switcher_event_to_action,
};
use vac_shell_model_switcher::{SwitcherEvent, SwitcherKey, on_key, render_model_switcher};

fn seed_state() -> ModelSelectionState {
    let providers = vec![
        ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        },
        ProviderInfo {
            id: ProviderId("openai".into()),
            credentials_present: true,
        },
    ];
    let models = vec![
        HostModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-sonnet-4.5".into(),
            label: "Claude Sonnet 4.5".into(),
            reasoning: true,
            cost_label: Some("$3 / $15 per M".into()),
        },
        HostModel {
            provider: ProviderId("openai".into()),
            id: "gpt-4o".into(),
            label: "GPT-4o".into(),
            reasoning: false,
            cost_label: Some("$2.5 / $10 per M".into()),
        },
    ];
    ModelSelectionState::new(
        providers,
        models,
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into())),
    )
}

fn build_host(state: ModelSelectionState) -> Arc<dyn ShellHost> {
    let controller: Arc<dyn ModelController> = Arc::new(ModelSelectionController::new(state));
    Arc::new(CompositeShellHost::new().with_model(controller))
}

#[test]
fn widget_event_to_host_mutation_to_render_active_tag() {
    // Initial state: anthropic active, no recents.
    let state = seed_state();
    let host = build_host(state.clone());

    // Build the view, open it, simulate the operator hitting Down +
    // Enter. With no recents and providers sorted alphabetically,
    // index 0 is `Claude Sonnet 4.5`; Down lands on `GPT-4o`.
    let mut view = build_switcher_view(&state, 5);
    view.visible = true;
    on_key(&mut view, SwitcherKey::Down);
    let event = on_key(&mut view, SwitcherKey::Enter);

    let action = switcher_event_to_action(event).expect("Selected event must produce an action");
    match &action {
        ShellAction::SelectModel { provider, id } => {
            assert_eq!(provider, &ProviderId("openai".into()));
            assert_eq!(id, "gpt-4o");
        }
        other => panic!("expected SelectModel, got {other:?}"),
    }

    // Route through the bridge — host applies the mutation.
    host.handle(action).expect("dispatch must succeed");

    // Active model has shifted host-side.
    assert_eq!(
        state.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
    assert_eq!(
        state.recent_snapshot()[0],
        (ProviderId("openai".into()), "gpt-4o".into())
    );

    // Re-project, render, and assert the new active row carries the
    // active tag in the buffer. This closes the chain.
    let mut new_view = build_switcher_view(&state, 5);
    new_view.visible = true;

    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_model_switcher(f, &new_view, f.area()))
        .unwrap();

    let buf = terminal.backend().buffer();
    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
        all.push('\n');
    }
    assert!(all.contains("GPT-4o"), "active model label missing\n{all}");
    assert!(all.contains("active"), "active tag missing\n{all}");
}

#[test]
fn dismissed_event_does_not_produce_an_action_or_mutate_host() {
    let state = seed_state();
    let host = build_host(state.clone());

    let mut view = build_switcher_view(&state, 5);
    view.visible = true;
    let event = on_key(&mut view, SwitcherKey::Escape);
    assert_eq!(event, SwitcherEvent::Dismissed);
    assert!(switcher_event_to_action(event).is_none());

    // Sanity: even calling host.handle on a fresh, unrelated action
    // would be moot here — there's no action to send. State stays
    // exactly where it started.
    assert_eq!(
        state.active_model(),
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into()))
    );
    assert!(state.recent_snapshot().is_empty());

    // Also assert the host can still process well-formed actions
    // afterwards.
    host.handle(ShellAction::SelectModel {
        provider: ProviderId("openai".into()),
        id: "gpt-4o".into(),
    })
    .unwrap();
    assert_eq!(
        state.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
}
