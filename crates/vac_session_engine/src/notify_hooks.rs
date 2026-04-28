use std::path::PathBuf;
use vac_session_primitives::hooks::{HookEvent, HookStore, HookSandbox, exec_hook_sandboxed};

/// C12 Notify hook execution path.
/// Loads the `.vac/hooks.json` dynamically and fires matching hooks
/// asynchronously. Execution is timeout-bounded by the sandbox,
/// and payloads are kept minimal (redacted) to prevent leakages.
pub fn fire_notification_hook(
    project_root: PathBuf,
    event: HookEvent,
    payload: Option<String>,
) {
    tokio::spawn(async move {
        let store = match HookStore::load(&project_root).await {
            Ok(s) => s,
            Err(_) => return,
        };
        let entries: Vec<_> = store.matches(event, "").into_iter().cloned().collect();
        
        for entry in entries {
            let mut sandbox = HookSandbox::operator_default();
            if let Some(ref mut allowlist) = sandbox.env_allowlist {
                allowlist.push("VAC_HOOK_EVENT".to_string());
                allowlist.push("VAC_HOOK_PAYLOAD".to_string());
            }
            
            let mut extra_env = std::collections::HashMap::new();
            extra_env.insert("VAC_HOOK_EVENT".to_string(), format!("{:?}", event));
            if let Some(ref p) = payload {
                extra_env.insert("VAC_HOOK_PAYLOAD".to_string(), p.clone());
            }
            
            let _ = exec_hook_sandboxed(&entry, &sandbox, Some(extra_env)).await;
        }
    });
}
