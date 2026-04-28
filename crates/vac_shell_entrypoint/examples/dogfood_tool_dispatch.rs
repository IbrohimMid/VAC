//! D8 — opt-in dogfood loop with **live** tool dispatch.
//!
//! This example wires a real `VacToolDispatcher` over a
//! freshly-seeded `vac_tools::ToolRegistry` (containing only
//! the read-only `GlobTool`). It also attaches an empty
//! `CompositeGate` so the D8 pre-flight check passes — hosts
//! that need real policy/hook gating compose them on the
//! gate before passing it in.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p vac_shell_entrypoint --example dogfood_tool_dispatch
//! ```
//!
//! The default dogfood example (`--example dogfood`) keeps the
//! engine on `UnsupportedDispatcher`. This file exists so an
//! operator can opt into live tool dispatch explicitly without
//! turning it on for every dogfood session.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use vac_session_engine::CompositeGate;
use vac_shell_host_vac_command_adapter::{AdapterConfig, VacCommandExecutorAdapter};
use vac_shell_host_vac_tool_dispatcher::VacToolDispatcher;
use vac_tools::ToolRegistry;
use vac_tools::registry::ToolContext;

fn main() -> ExitCode {
    let root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dogfood_tool_dispatch: cannot resolve cwd: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Build the tool registry on a small private runtime —
    // ToolRegistry::register is async. The runtime is dropped
    // before we hand control to the runtime loop.
    let registry = match build_registry_blocking(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dogfood_tool_dispatch: registry build failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ctx = Arc::new(ToolContext::new(root.clone()));
    let dispatcher = Arc::new(VacToolDispatcher::new(
        Arc::clone(&registry),
        Arc::clone(&ctx),
    ));
    // Empty CompositeGate satisfies the D8 pre-flight check —
    // the engine still runs every tool through the gate, but
    // an empty composite returns Allow for everything. Hosts
    // that need policy/hook enforcement compose them here.
    let gate = Arc::new(CompositeGate::new());

    let app = vac_shell_entrypoint::build_shell_app(&root);
    let adapter_cfg = AdapterConfig::dogfood(root.clone()).with_tool_dispatcher(dispatcher, gate);
    let adapter = VacCommandExecutorAdapter::new(adapter_cfg);
    let ctx_runtime =
        vac_shell_runtime_loop::ShellRuntimeContext::new(app).with_executor(Arc::new(adapter));
    match vac_shell_runtime_loop::run_shell_loop(
        ctx_runtime,
        vac_shell_runtime_loop::ShellLoopOptions::default(),
    ) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("dogfood_tool_dispatch: shell loop failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn build_registry_blocking(_root: &PathBuf) -> Result<Arc<ToolRegistry>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async {
        let registry = ToolRegistry::new();
        registry
            .register(vac_tools::builtin::glob::GlobTool)
            .await
            .map_err(|e| format!("register glob: {e}"))?;
        Ok::<_, String>(Arc::new(registry))
    })
}
