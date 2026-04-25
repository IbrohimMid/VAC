//! Slice 19 — host-side status projection.
//!
//! Reads `ShellComposition` (surface state, approval queue, model
//! selection state) plus optional host-supplied tags (cwd, git
//! branch, last error, running tasks) and produces the
//! `ShellStatusView` the status bar widget renders.

use vac_shell_composition::ShellComposition;
use vac_shell_contracts::ShellStatusView;
use vac_shell_host_surface::Surface;

#[derive(Debug, Clone, Default)]
pub struct StatusInputs {
    pub cwd: String,
    pub git_branch: Option<String>,
    pub running_tasks: usize,
    pub last_error: Option<String>,
}

pub fn project_status(comp: &ShellComposition, inputs: &StatusInputs) -> ShellStatusView {
    let surface = match comp.surface_state.current() {
        Surface::Chat => "chat",
        Surface::Runtime => "runtime",
    };
    // Slice 20.1 fix — `pending_approvals` means "rows still in the
    // queue awaiting submission". A row's per-status flag
    // (`Approved` / `Rejected`) is the operator's tentative
    // decision; rows leave the queue only when `submit_all` drains
    // it. So the pending count == queue length.
    let pending = comp.approval_queue.snapshot().len();
    let model_label = comp.model_state.active_model().map(|(p, id)| {
        format!("{} / {}", p.0, id)
    });
    ShellStatusView {
        surface: Some(surface.into()),
        model_label,
        cwd: inputs.cwd.clone(),
        git_branch: inputs.git_branch.clone(),
        pending_approvals: pending,
        running_tasks: inputs.running_tasks,
        last_error: inputs.last_error.clone(),
    }
}
