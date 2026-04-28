//! Helper functions extracted from `update.rs` to keep the top-level dispatcher
//! under the Wave 1 exit-criteria 600-line limit.
//!
//! All helpers are re-exported from `crate::update` so external callers
//! (`controller.rs`, `handlers::input_commands`) keep working without churn.

use crate::app::AppState;

pub fn estimate_context_percent(model: Option<&crate::types::Model>, tokens_used: u64) -> f32 {
    if tokens_used == 0 {
        return 0.0;
    }
    let window: u64 = match model {
        Some(m) => {
            let id = m.id.to_lowercase();
            let name = m.name.to_lowercase();
            if id.contains("claude")
                || name.contains("claude")
                || id.contains("sonnet")
                || id.contains("opus")
                || id.contains("haiku")
            {
                200_000
            } else if id.contains("gpt-4o") || name.contains("gpt-4o") {
                128_000
            } else if id.contains("gpt-4") || name.contains("gpt-4") {
                8_192
            } else if id.contains("gemini") {
                1_000_000
            } else {
                200_000
            }
        }
        None => 200_000,
    };
    ((tokens_used as f64 / window as f64) * 100.0).clamp(0.0, 100.0) as f32
}

/// Open the ask-user popup driven by the ASK_USER tool call.
pub fn open_ask_user_popup(state: &mut AppState, tc: &crate::types::ToolCall) {
    let args = crate::services::ask_user::parse_args(&tc.function.arguments);
    // Default policy: allow free-text iff the caller opts in OR no options
    // were supplied (otherwise the user would have no way to answer).
    let (question, options, allow_free_text, kind, metadata) = match args {
        Some(a) => {
            let k = a.effective_kind();
            let free = matches!(
                k,
                crate::services::ask_user::AskUserQuestionKind::FreeText
                    | crate::services::ask_user::AskUserQuestionKind::Mixed
            ) || a.allow_free_text
                || a.options.is_empty();
            (Some(a.question), a.options, free, k, a.metadata)
        }
        None => (
            Some("The assistant needs more information.".to_string()),
            Vec::new(),
            true,
            crate::services::ask_user::AskUserQuestionKind::FreeText,
            std::collections::HashMap::new(),
        ),
    };
    state.layout.ask_user.question = question;
    state.layout.ask_user.options = options;
    state.layout.ask_user.selected = 0;
    state.layout.ask_user.input.clear();
    state.layout.ask_user.allow_free_text = allow_free_text;
    state.layout.ask_user.question_kind = kind;
    state.layout.ask_user.metadata = metadata;
    state.layout.ask_user.multi_selected.clear();
    state.layout.ask_user.filter.clear();
    state.layout.ask_user.search_active = false;
    state.layout.ask_user.scroll = 0;
    state.layout.ask_user.tool_call_id = Some(tc.id.clone());
    crate::overlay::open_overlay(state, crate::overlay::OverlayId::AskUser);
    state.push_activity(
        crate::app::ActivityKind::Approval,
        "Assistant requested input",
    );
}

pub fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read"
            | "file_write"
            | "file_edit"
            | "glob"
            | "grep"
            | "search"
            | "vil_knowledge"
            | "vil_diagnostics"
            | "vil_status"
            | "vil_lsp_query"
            | "vil_ir_diff"
            | "vil_audit"
            | "vil_plumbing"
            | "vil_repair"
    )
}

pub fn is_vil_tool(tool_name: &str) -> bool {
    tool_name.starts_with("vil_")
}

pub(crate) fn truncate_banner_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars()
        .take(max_chars.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

pub fn classify_critical_banner(
    text: &str,
) -> Option<(
    crate::services::banner::BannerStyle,
    crate::services::banner::BannerSeverity,
)> {
    let lower = text.to_ascii_lowercase();
    let tls_related = lower.contains("tls")
        || lower.contains("certificate")
        || lower.contains("ca file")
        || lower.contains("mtls")
        || lower.contains("server_name")
        || lower.contains("identity");

    if lower.contains("warden blocked")
        || lower.contains("trust/policy violation")
        || lower.contains("permission denied")
        || lower.contains("blocked:")
        || lower.contains("blocked by")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("failed to connect mcp server") || lower.contains("mcp error") {
        return Some(if tls_related {
            (
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            )
        } else {
            (
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            )
        });
    }

    if lower.contains("agent loop exceeded")
        || lower.contains("iterations without completing")
        || lower.contains("max iterations")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Blocking,
        ));
    }

    if lower.contains("rate limited") || lower.contains("retry after") || lower.contains("429") {
        return Some((
            crate::services::banner::BannerStyle::Warning,
            crate::services::banner::BannerSeverity::Suggested,
        ));
    }

    if lower.contains("all providers in fallback chain failed")
        || lower.contains("maximum retry attempts")
        || lower.contains("max retry")
    {
        return Some((
            crate::services::banner::BannerStyle::Error,
            crate::services::banner::BannerSeverity::Suggested,
        ));
    }

    None
}

pub(crate) fn push_banner_direct(
    state: &mut AppState,
    text: String,
    style: crate::services::banner::BannerStyle,
    severity: crate::services::banner::BannerSeverity,
) {
    let msg = crate::services::banner::BannerMessage::new(text, style).with_severity(severity);
    state.layout.banner.queue.push(msg);
    state.layout.banner.message = state.layout.banner.queue.current().cloned();
}

pub(crate) fn policy_gate_allows_shell_command(state: &mut AppState, cmd: &str) -> bool {
    let Ok(config) = vac_core::VacConfig::load_with_fallback(&state.core.project_root) else {
        return true;
    };
    let Some(action) = vac_core::policy_gate::classify_shell_command(cmd) else {
        return true;
    };
    let decision = vac_core::policy_gate::evaluate(
        &config.policy_gate,
        action,
        state.vil_domain.vil.last_score,
    );
    match decision {
        vac_core::policy_gate::PolicyGateDecision::Allow => true,
        vac_core::policy_gate::PolicyGateDecision::Warn(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            );
            true
        }
        vac_core::policy_gate::PolicyGateDecision::Block(msg) => {
            push_banner_direct(
                state,
                truncate_banner_text(&msg, 140),
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            );
            false
        }
    }
}
