use std::path::PathBuf;
use vac_session_primitives::hooks::{HookEvent, HookSandbox, HookStore, exec_hook_sandboxed};

pub(crate) fn redact_notification_payload(raw: &str) -> String {
    const MAX_LEN: usize = 512;
    let mut s = raw.trim().to_string();
    if s.len() > MAX_LEN {
        s.truncate(MAX_LEN);
    }
    let re_keyval =
        regex::Regex::new(r#"(?i)\b(api[_-]?key|token|secret|password)\b\s*[:=]\s*([^\s"']+)"#)
            .ok();
    if let Some(re) = re_keyval {
        s = re
            .replace_all(&s, |caps: &regex::Captures| {
                format!("{}=<redacted>", &caps[1])
            })
            .to_string();
    }
    let re_sk = regex::Regex::new(r"\bsk-[A-Za-z0-9]{8,}\b").ok();
    if let Some(re) = re_sk {
        s = re.replace_all(&s, "sk-<redacted>").to_string();
    }
    s
}

/// C12 Notify hook execution path.
/// Loads the `.vac/hooks.json` dynamically and fires matching hooks
/// asynchronously. Execution is timeout-bounded by the sandbox,
/// and payloads are kept minimal (redacted) to prevent leakages.
pub fn fire_notification_hook(project_root: PathBuf, event: HookEvent, payload: Option<String>) {
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
                extra_env.insert(
                    "VAC_HOOK_PAYLOAD".to_string(),
                    redact_notification_payload(p),
                );
            }

            let _ = exec_hook_sandboxed(&entry, &sandbox, Some(extra_env)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_like_payloads() {
        let raw = "token=sk-1234567890abcdef API_KEY=xyz password: hunter2";
        let out = redact_notification_payload(raw);
        assert!(!out.contains("sk-1234567890abcdef"));
        assert!(!out.contains("xyz"));
        assert!(!out.contains("hunter2"));
        assert!(out.contains("<redacted>"));
    }
}
