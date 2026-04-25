//! Slice 10 — `ShellComposition` host bring-up.
//!
//! Single struct that wires all host-owned shell pieces together:
//! `VacPaths`, the unified `ShellHost`, the command registry, and
//! the three pieces of mutable state (surface, approval queue,
//! model selection). The TUI event loop consumes this and never
//! touches the underlying host crates directly.
//!
//! Boot flow:
//!
//! ```text
//! VacPathsImpl::new(project_root)
//!   → vac_paths_persistor(paths)
//!   → boot_selection_state(providers, models, fallback_active, persistor)
//!   → CompositeShellHost::new()
//!         .with_surface(SurfaceStateController)
//!         .with_approval(ApprovalQueueController)
//!         .with_model(ModelSelectionController)
//!   → command registry seeded
//!   → ShellComposition { paths, host, command_registry,
//!                         surface_state, approval_queue, model_state }
//! ```
//!
//! No provider API. No semantic runtime switching. No donor types.

use std::sync::Arc;

use vac_shell_bridge::{
    ApprovalController, CompositeShellHost, DispatchError, InMemoryCommandRegistry,
    ModelController, ShellHost, SurfaceController, VacCommandRegistry,
};
use vac_shell_contracts::{ProviderId, ShellCommandSpec, VacPaths};
use vac_shell_host_approval::{ApprovalQueue, ApprovalQueueController};
use vac_shell_host_model::{
    HostModel, ModelSelectionController, ModelSelectionState, ProviderInfo,
    boot_selection_state, vac_paths_persistor,
};
use vac_shell_host_surface::{Surface, SurfaceState, SurfaceStateController};

/// Live shell-host composition — handed to the TUI event loop.
#[derive(Clone)]
pub struct ShellComposition {
    pub paths: Arc<dyn VacPaths>,
    pub host: Arc<dyn ShellHost>,
    pub command_registry: Arc<dyn VacCommandRegistry>,
    pub surface_state: SurfaceState,
    pub approval_queue: ApprovalQueue,
    pub model_state: ModelSelectionState,
}

/// Builder for `ShellComposition`. Hosts call `boot()` once with
/// the provider/model registry and the initial slash-command list;
/// boot also restores any previously-persisted model selection from
/// disk through the supplied `VacPaths`.
pub struct ShellCompositionBuilder {
    paths: Arc<dyn VacPaths>,
    providers: Vec<ProviderInfo>,
    models: Vec<HostModel>,
    fallback_active: Option<(ProviderId, String)>,
    initial_surface: Surface,
    commands: Vec<ShellCommandSpec>,
}

impl ShellCompositionBuilder {
    pub fn new(paths: Arc<dyn VacPaths>) -> Self {
        Self {
            paths,
            providers: Vec::new(),
            models: Vec::new(),
            fallback_active: None,
            initial_surface: Surface::Chat,
            commands: Vec::new(),
        }
    }

    pub fn with_providers(mut self, providers: Vec<ProviderInfo>) -> Self {
        self.providers = providers;
        self
    }

    pub fn with_models(mut self, models: Vec<HostModel>) -> Self {
        self.models = models;
        self
    }

    pub fn with_fallback_active(mut self, key: Option<(ProviderId, String)>) -> Self {
        self.fallback_active = key;
        self
    }

    pub fn with_initial_surface(mut self, surface: Surface) -> Self {
        self.initial_surface = surface;
        self
    }

    pub fn with_commands(mut self, specs: Vec<ShellCommandSpec>) -> Self {
        self.commands = specs;
        self
    }

    pub fn boot(self) -> Result<ShellComposition, DispatchError> {
        let surface_state = SurfaceState::new(self.initial_surface);
        let surface_ctrl: Arc<dyn SurfaceController> =
            Arc::new(SurfaceStateController::new(surface_state.clone()));

        let approval_queue = ApprovalQueue::new();
        let approval_ctrl: Arc<dyn ApprovalController> =
            Arc::new(ApprovalQueueController::new(approval_queue.clone()));

        let persistor = Arc::new(vac_paths_persistor(self.paths.as_ref()));
        let model_state = boot_selection_state(
            self.providers,
            self.models,
            self.fallback_active,
            persistor,
        )?;
        let model_ctrl: Arc<dyn ModelController> =
            Arc::new(ModelSelectionController::new(model_state.clone()));

        let host: Arc<dyn ShellHost> = Arc::new(
            CompositeShellHost::new()
                .with_surface(surface_ctrl)
                .with_approval(approval_ctrl)
                .with_model(model_ctrl),
        );

        let command_registry: Arc<dyn VacCommandRegistry> =
            Arc::new(InMemoryCommandRegistry::new(self.commands));

        Ok(ShellComposition {
            paths: self.paths,
            host,
            command_registry,
            surface_state,
            approval_queue,
            model_state,
        })
    }
}
