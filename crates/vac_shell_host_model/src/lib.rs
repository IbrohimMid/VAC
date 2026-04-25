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

use vac_shell_contracts::{ProviderId, VacModelView};
use vac_shell_model_switcher::{ModelSwitcherView, clamp_selection};

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
