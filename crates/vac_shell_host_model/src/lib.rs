//! Step 2 slice 8.2 — host-side **read-only** model projection.
//!
//! The model switcher widget consumes `Vec<VacModelView>`. Slice 8.2
//! defines the host adapter that builds those views from a VAC-side
//! source (config, registry, future trait impl). This crate stays
//! strictly read-only: no provider switching, no active-model write,
//! no secret manager, no API mutation. All of that lives in slice 9
//! behind a host-side mutation seam, gated by review.
//!
//! # Boundary
//!
//! * Depends on `vac_shell_contracts` (DTO surface) and
//!   `vac_shell_model_switcher` (builds the `ModelSwitcherView`).
//! * Does NOT depend on `vac_core`, `vac_session_engine`,
//!   `vac_tui_runtime`, `stakai`, or the donor — and must not until
//!   a real VAC model registry trait is wired in here.
//! * Concrete VAC config wiring (e.g. `VacConfig.llm.providers`) is
//!   plugged in via the [`ModelSource`] trait so this crate keeps a
//!   tight dep graph; tests use the bundled [`InMemoryModelSource`].

use std::sync::{Arc, RwLock};

use vac_shell_bridge::{DispatchError, ModelController, ShellAction};
use vac_shell_contracts::{ProviderId, VacModelView};
use vac_shell_model_switcher::{ModelSwitcherView, SwitcherEvent, clamp_selection};

/// Provider summary the host source advertises. Mirrors the bits the
/// UI cares about; richer provider state stays host-internal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: ProviderId,
    /// True when the host has credentials wired for this provider.
    pub credentials_present: bool,
}

/// Host-side model record — flat, allocation-friendly, never carries
/// `stakai::Model` or any donor type. Hosts that already speak in
/// VAC config types map them onto this at the host boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostModel {
    pub provider: ProviderId,
    pub id: String,
    pub label: String,
    pub reasoning: bool,
    /// Pre-formatted cost label (host owns currency / pricing logic).
    pub cost_label: Option<String>,
}

/// Read-only model registry. Implementations live host-side and may
/// wrap a VAC `Config` snapshot, an authentication store, or a
/// background poller — none of those types appear here.
pub trait ModelSource {
    fn providers(&self) -> Vec<ProviderInfo>;
    fn models(&self) -> Vec<HostModel>;
    fn active_model(&self) -> Option<(ProviderId, String)>;
    fn recent_models(&self, limit: usize) -> Vec<(ProviderId, String)>;
    fn pinned_provider(&self) -> Option<ProviderId>;
}

/// Project a `ModelSource` snapshot into the `Vec<VacModelView>`
/// the model switcher widget renders. Pure read — never touches the
/// source after collecting.
pub fn project_models(source: &dyn ModelSource) -> Vec<VacModelView> {
    let providers = source.providers();
    let active = source.active_model();
    source
        .models()
        .into_iter()
        .map(|m| {
            let credentials_present = providers
                .iter()
                .find(|p| p.id == m.provider)
                .map(|p| p.credentials_present)
                .unwrap_or(false);
            let active = active
                .as_ref()
                .map(|(p, id)| p == &m.provider && id == &m.id)
                .unwrap_or(false);
            VacModelView {
                provider: m.provider,
                id: m.id,
                label: m.label,
                active,
                credentials_present,
                reasoning: m.reasoning,
                cost_label: m.cost_label,
            }
        })
        .collect()
}

/// Build a fully populated [`ModelSwitcherView`] from a source. The
/// returned view is ready for `render_model_switcher` and has its
/// selection clamped against the navigation order so a caller that
/// rebuilds the view between ticks doesn't end up with an
/// out-of-range highlight before the next key event.
pub fn build_switcher_view(source: &dyn ModelSource, recents_limit: usize) -> ModelSwitcherView {
    let mut view = ModelSwitcherView::new(project_models(source))
        .with_recent(source.recent_models(recents_limit));
    if let Some(pinned) = source.pinned_provider() {
        view = view.with_pinned_provider(pinned);
    }
    clamp_selection(&mut view);
    view
}

// =====================================================================
// In-memory test fixture
// =====================================================================

/// Minimal `ModelSource` impl for tests and host bring-up. Hosts
/// wiring against real VAC config replace this with their own impl.
#[derive(Debug, Clone, Default)]
pub struct InMemoryModelSource {
    pub providers: Vec<ProviderInfo>,
    pub models: Vec<HostModel>,
    pub active: Option<(ProviderId, String)>,
    pub recent: Vec<(ProviderId, String)>,
    pub pinned: Option<ProviderId>,
}

impl InMemoryModelSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(mut self, id: &str, credentials_present: bool) -> Self {
        self.providers.push(ProviderInfo {
            id: ProviderId(id.into()),
            credentials_present,
        });
        self
    }

    pub fn with_model(
        mut self,
        provider: &str,
        id: &str,
        label: &str,
        reasoning: bool,
        cost_label: Option<&str>,
    ) -> Self {
        self.models.push(HostModel {
            provider: ProviderId(provider.into()),
            id: id.into(),
            label: label.into(),
            reasoning,
            cost_label: cost_label.map(|s| s.to_string()),
        });
        self
    }

    pub fn with_active(mut self, provider: &str, id: &str) -> Self {
        self.active = Some((ProviderId(provider.into()), id.into()));
        self
    }

    pub fn with_recent(mut self, provider: &str, id: &str) -> Self {
        self.recent.push((ProviderId(provider.into()), id.into()));
        self
    }

    pub fn with_pinned(mut self, provider: &str) -> Self {
        self.pinned = Some(ProviderId(provider.into()));
        self
    }
}

impl ModelSource for InMemoryModelSource {
    fn providers(&self) -> Vec<ProviderInfo> {
        self.providers.clone()
    }

    fn models(&self) -> Vec<HostModel> {
        self.models.clone()
    }

    fn active_model(&self) -> Option<(ProviderId, String)> {
        self.active.clone()
    }

    fn recent_models(&self, limit: usize) -> Vec<(ProviderId, String)> {
        self.recent.iter().take(limit).cloned().collect()
    }

    fn pinned_provider(&self) -> Option<ProviderId> {
        self.pinned.clone()
    }
}

// =====================================================================
// Slice 9 — host-side mutation seam
// =====================================================================
//
// Mutable VAC-side model state. Validates incoming
// `ShellAction::SelectModel` against the known providers/models and
// the credentials advertised by the host source. Recents are kept
// newest-first with deduplication. **No persistence yet** — this is
// an in-memory mutation seam; persistent VAC config writes are a
// follow-up slice.

const RECENTS_CAP: usize = 20;

#[derive(Debug, Default)]
struct ModelSelectionInner {
    providers: Vec<ProviderInfo>,
    models: Vec<HostModel>,
    active: Option<(ProviderId, String)>,
    recent: Vec<(ProviderId, String)>,
    pinned: Option<ProviderId>,
}

/// Shared, mutable model-selection state. Cheap to clone (`Arc`
/// inside); thread-safe.
#[derive(Debug, Clone, Default)]
pub struct ModelSelectionState {
    inner: Arc<RwLock<ModelSelectionInner>>,
}

impl ModelSelectionState {
    pub fn new(
        providers: Vec<ProviderInfo>,
        models: Vec<HostModel>,
        active: Option<(ProviderId, String)>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ModelSelectionInner {
                providers,
                models,
                active,
                recent: Vec::new(),
                pinned: None,
            })),
        }
    }

    pub fn with_recent(self, recent: Vec<(ProviderId, String)>) -> Self {
        {
            let mut inner = self.inner.write().expect("model state lock poisoned");
            inner.recent = recent;
        }
        self
    }

    pub fn with_pinned(self, pinned: ProviderId) -> Self {
        {
            let mut inner = self.inner.write().expect("model state lock poisoned");
            inner.pinned = Some(pinned);
        }
        self
    }

    pub fn active_model(&self) -> Option<(ProviderId, String)> {
        self.inner.read().expect("model state lock poisoned").active.clone()
    }

    pub fn recent_snapshot(&self) -> Vec<(ProviderId, String)> {
        self.inner.read().expect("model state lock poisoned").recent.clone()
    }

    /// Apply a selection. Validates provider is known, model is
    /// known, and the provider has credentials present. On success,
    /// updates `active` and pushes onto the recents list (dedup,
    /// newest-first, capped at `RECENTS_CAP`).
    pub fn select_model(
        &self,
        provider: &ProviderId,
        id: &str,
    ) -> Result<(), DispatchError> {
        let mut inner = self.inner.write().expect("model state lock poisoned");

        let prov = inner
            .providers
            .iter()
            .find(|p| &p.id == provider)
            .cloned()
            .ok_or_else(|| {
                DispatchError::Host(format!("unknown model provider: {}", provider.0))
            })?;

        if !inner
            .models
            .iter()
            .any(|m| &m.provider == provider && m.id == id)
        {
            return Err(DispatchError::Host(format!(
                "unknown model: {}/{}",
                provider.0, id
            )));
        }

        if !prov.credentials_present {
            return Err(DispatchError::Host(format!(
                "model provider has no credentials: {}",
                provider.0
            )));
        }

        let key = (provider.clone(), id.to_string());
        inner.active = Some(key.clone());
        inner.recent.retain(|p| p != &key);
        inner.recent.insert(0, key);
        if inner.recent.len() > RECENTS_CAP {
            inner.recent.truncate(RECENTS_CAP);
        }
        Ok(())
    }
}

impl ModelSource for ModelSelectionState {
    fn providers(&self) -> Vec<ProviderInfo> {
        self.inner.read().expect("model state lock poisoned").providers.clone()
    }

    fn models(&self) -> Vec<HostModel> {
        self.inner.read().expect("model state lock poisoned").models.clone()
    }

    fn active_model(&self) -> Option<(ProviderId, String)> {
        self.inner.read().expect("model state lock poisoned").active.clone()
    }

    fn recent_models(&self, limit: usize) -> Vec<(ProviderId, String)> {
        self.inner
            .read()
            .expect("model state lock poisoned")
            .recent
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    fn pinned_provider(&self) -> Option<ProviderId> {
        self.inner.read().expect("model state lock poisoned").pinned.clone()
    }
}

/// Concrete `ModelController` over a `ModelSelectionState`. The
/// bridge dispatches `ShellAction::SelectModel` here; this is the
/// only place active-model mutation happens in the new shell stack.
#[derive(Debug, Clone)]
pub struct ModelSelectionController {
    state: ModelSelectionState,
}

impl ModelSelectionController {
    pub fn new(state: ModelSelectionState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> ModelSelectionState {
        self.state.clone()
    }
}

impl ModelController for ModelSelectionController {
    fn select_model(
        &self,
        provider: &ProviderId,
        id: &str,
    ) -> Result<(), DispatchError> {
        self.state.select_model(provider, id)
    }
}

/// Map a model-switcher widget event into a `ShellAction` the
/// bridge can route. Only `Selected` produces an action; other
/// events stay UI-local.
pub fn switcher_event_to_action(event: SwitcherEvent) -> Option<ShellAction> {
    match event {
        SwitcherEvent::Selected { provider, id } => {
            Some(ShellAction::SelectModel { provider, id })
        }
        SwitcherEvent::Dismissed | SwitcherEvent::Consumed | SwitcherEvent::Ignored => None,
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

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
            ProviderInfo {
                id: ProviderId("kilo".into()),
                credentials_present: false,
            },
        ];
        let models = vec![
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("openai".into()),
                id: "gpt-4o".into(),
                label: "GPT-4o".into(),
                reasoning: false,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("kilo".into()),
                id: "kilo-auto".into(),
                label: "Kilo Auto".into(),
                reasoning: false,
                cost_label: None,
            },
        ];
        ModelSelectionState::new(
            providers,
            models,
            Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into())),
        )
    }

    #[test]
    fn select_model_updates_active_model() {
        let state = seed_state();
        state
            .select_model(&ProviderId("openai".into()), "gpt-4o")
            .unwrap();
        assert_eq!(
            state.active_model(),
            Some((ProviderId("openai".into()), "gpt-4o".into()))
        );
    }

    #[test]
    fn select_model_pushes_recent_to_front_and_dedups() {
        let state = seed_state();
        state
            .select_model(&ProviderId("openai".into()), "gpt-4o")
            .unwrap();
        state
            .select_model(&ProviderId("anthropic".into()), "claude-sonnet-4.5")
            .unwrap();
        // Re-select gpt-4o → should move to front, not duplicate.
        state
            .select_model(&ProviderId("openai".into()), "gpt-4o")
            .unwrap();
        let recent = state.recent_snapshot();
        assert_eq!(recent.len(), 2, "dedup must keep one entry per (provider, id)");
        assert_eq!(recent[0], (ProviderId("openai".into()), "gpt-4o".into()));
        assert_eq!(
            recent[1],
            (ProviderId("anthropic".into()), "claude-sonnet-4.5".into())
        );
    }

    #[test]
    fn select_model_rejects_unknown_provider() {
        let state = seed_state();
        let err = state
            .select_model(&ProviderId("ghost".into()), "x")
            .unwrap_err();
        match err {
            DispatchError::Host(msg) => assert!(msg.contains("unknown model provider")),
            other => panic!("expected Host(...), got {other:?}"),
        }
    }

    #[test]
    fn select_model_rejects_unknown_model() {
        let state = seed_state();
        let err = state
            .select_model(&ProviderId("openai".into()), "no-such-model")
            .unwrap_err();
        match err {
            DispatchError::Host(msg) => assert!(msg.contains("unknown model")),
            other => panic!("expected Host(...), got {other:?}"),
        }
    }

    #[test]
    fn select_model_rejects_provider_without_credentials() {
        let state = seed_state();
        let err = state
            .select_model(&ProviderId("kilo".into()), "kilo-auto")
            .unwrap_err();
        match err {
            DispatchError::Host(msg) => assert!(msg.contains("no credentials")),
            other => panic!("expected Host(...), got {other:?}"),
        }
        // Active must not have moved.
        assert_eq!(
            state.active_model(),
            Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into()))
        );
    }

    #[test]
    fn project_models_reflects_new_active_after_selection() {
        let state = seed_state();
        state
            .select_model(&ProviderId("openai".into()), "gpt-4o")
            .unwrap();
        let views = project_models(&state);
        let actives: Vec<&str> = views
            .iter()
            .filter(|m| m.active)
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(actives, vec!["gpt-4o"]);
    }

    #[test]
    fn switcher_event_to_action_maps_only_selected() {
        let action = switcher_event_to_action(SwitcherEvent::Selected {
            provider: ProviderId("openai".into()),
            id: "gpt-4o".into(),
        });
        match action {
            Some(ShellAction::SelectModel { provider, id }) => {
                assert_eq!(provider, ProviderId("openai".into()));
                assert_eq!(id, "gpt-4o");
            }
            other => panic!("expected SelectModel, got {other:?}"),
        }
        assert!(switcher_event_to_action(SwitcherEvent::Dismissed).is_none());
        assert!(switcher_event_to_action(SwitcherEvent::Consumed).is_none());
        assert!(switcher_event_to_action(SwitcherEvent::Ignored).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> InMemoryModelSource {
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
            .with_model("openai", "gpt-4o", "GPT-4o", false, Some("$2.5 / $10 per M"))
            .with_model("kilo", "kilo-auto", "Kilo Auto", false, None)
    }

    #[test]
    fn project_models_marks_active_and_credentials() {
        let src = fixture().with_active("anthropic", "claude-sonnet-4.5");
        let views = project_models(&src);
        let active: Vec<&VacModelView> = views.iter().filter(|m| m.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "claude-sonnet-4.5");

        let kilo = views.iter().find(|m| m.id == "kilo-auto").unwrap();
        assert!(!kilo.credentials_present, "kilo provider has no creds");
        let claude = views.iter().find(|m| m.id == "claude-sonnet-4.5").unwrap();
        assert!(claude.credentials_present);
    }

    #[test]
    fn project_models_carries_reasoning_and_cost_label_unchanged() {
        let views = project_models(&fixture());
        let claude = views.iter().find(|m| m.id == "claude-sonnet-4.5").unwrap();
        assert!(claude.reasoning);
        assert_eq!(claude.cost_label.as_deref(), Some("$3 / $15 per M"));
        let kilo = views.iter().find(|m| m.id == "kilo-auto").unwrap();
        assert!(!kilo.reasoning);
        assert!(kilo.cost_label.is_none());
    }

    #[test]
    fn build_switcher_view_carries_recents_and_pinned_provider() {
        let src = fixture()
            .with_recent("openai", "gpt-4o")
            .with_pinned("kilo");
        let view = build_switcher_view(&src, 5);
        assert_eq!(
            view.recent,
            vec![(ProviderId("openai".into()), "gpt-4o".into())]
        );
        assert_eq!(view.pinned_provider, Some(ProviderId("kilo".into())));
    }

    #[test]
    fn build_switcher_view_clamps_initial_selection_to_zero() {
        let src = InMemoryModelSource::new();
        let view = build_switcher_view(&src, 5);
        assert!(view.models.is_empty());
        assert_eq!(view.selected, 0, "empty source must clamp to 0");
    }

    #[test]
    fn recent_limit_caps_returned_entries() {
        let src = fixture()
            .with_recent("openai", "gpt-4o")
            .with_recent("anthropic", "claude-sonnet-4.5")
            .with_recent("kilo", "kilo-auto");
        let view = build_switcher_view(&src, 2);
        assert_eq!(view.recent.len(), 2);
    }

    /// Drift tripwire — no donor / engine / runtime types may show up
    /// at this crate's public surface. Real dep proof is `cargo tree`.
    #[test]
    fn public_types_are_local_smoke_test() {
        fn assert_only_local<T: Sized>(_: T) {}
        assert_only_local(InMemoryModelSource::new());
    }
}
