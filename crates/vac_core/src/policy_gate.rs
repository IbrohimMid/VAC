use crate::config::PolicyGateConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyGateAction {
    Apply,
    Deploy,
    Merge,
}

impl PolicyGateAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Deploy => "deploy",
            Self::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyGateMode {
    Strict,
    Soft,
}

impl PolicyGateMode {
    /// Parse mode string. Unknown values return `None` so callers can
    /// fail-closed (treat as Strict) instead of silently degrading to Soft.
    pub fn parse(s: &str) -> Option<Self> {
        match normalize_key(s).as_str() {
            "strict" => Some(Self::Strict),
            "soft" => Some(Self::Soft),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyGateDecision {
    Allow,
    Warn(String),
    Block(String),
}

pub fn classify_shell_command(command: &str) -> Option<PolicyGateAction> {
    let c = command.trim();
    if c.is_empty() {
        return None;
    }
    let lower = c.to_lowercase();
    let tokens = lower.split_whitespace().collect::<Vec<_>>();
    let t0 = tokens.first().copied().unwrap_or("");
    let t1 = tokens.get(1).copied().unwrap_or("");
    let t2 = tokens.get(2).copied().unwrap_or("");

    if t0 == "git" && t1 == "merge" {
        return Some(PolicyGateAction::Merge);
    }
    if t0 == "gh" && t1 == "pr" && t2 == "merge" {
        return Some(PolicyGateAction::Merge);
    }
    if t0 == "kubectl" && t1 == "apply" {
        return Some(PolicyGateAction::Apply);
    }
    if t0 == "terraform" && t1 == "apply" {
        return Some(PolicyGateAction::Apply);
    }
    if t0 == "git" && t1 == "push" {
        return Some(PolicyGateAction::Deploy);
    }
    if t0 == "kubectl" && t1 == "rollout" {
        return Some(PolicyGateAction::Deploy);
    }
    if t0 == "helm" && t1 == "upgrade" {
        return Some(PolicyGateAction::Deploy);
    }
    if t0 == "docker" && t1 == "push" {
        return Some(PolicyGateAction::Deploy);
    }
    if t0 == "cargo" && t1 == "publish" {
        return Some(PolicyGateAction::Deploy);
    }

    None
}

pub fn evaluate(
    cfg: &PolicyGateConfig,
    action: PolicyGateAction,
    score: Option<f64>,
) -> PolicyGateDecision {
    if !is_action_gated(cfg, action) {
        return PolicyGateDecision::Allow;
    }

    let mode = PolicyGateMode::parse(&cfg.mode).unwrap_or_else(|| {
        tracing::warn!(
            mode = %cfg.mode,
            "policy_gate: unknown mode, defaulting to Strict (fail-closed)"
        );
        PolicyGateMode::Strict
    });
    match score {
        Some(s) => {
            if s < cfg.threshold {
                let msg = format!(
                    "Policy gate: aksi '{}' dibatasi karena validation score {:.2} < threshold {:.2}.",
                    action.as_str(),
                    s,
                    cfg.threshold
                );
                match mode {
                    PolicyGateMode::Strict => PolicyGateDecision::Block(msg),
                    PolicyGateMode::Soft => PolicyGateDecision::Warn(msg),
                }
            } else {
                PolicyGateDecision::Allow
            }
        }
        None => {
            let msg = format!(
                "Policy gate: validation score tidak tersedia untuk aksi '{}'.",
                action.as_str()
            );
            match mode {
                PolicyGateMode::Strict => PolicyGateDecision::Block(msg),
                PolicyGateMode::Soft => PolicyGateDecision::Warn(msg),
            }
        }
    }
}

fn is_action_gated(cfg: &PolicyGateConfig, action: PolicyGateAction) -> bool {
    if !cfg.enable {
        return false;
    }
    if cfg.actions.is_empty() {
        return true;
    }
    let needle = action.as_str();
    cfg.actions
        .iter()
        .map(|s| normalize_key(s))
        .any(|s| s == needle)
}

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase().replace(['_', ' '], "-")
}
