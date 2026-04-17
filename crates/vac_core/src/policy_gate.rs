use crate::config::PolicyGateConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyGateMode {
    Strict,
    Soft,
}

impl Default for PolicyGateMode {
    fn default() -> Self {
        Self::Soft
    }
}

impl PolicyGateMode {
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
    // Replace shell operators and subshell characters with space
    let sanitized = command.replace(
        &['$', '(', ')', '`', '&', '|', ';', '<', '>', '{', '}'][..],
        " ",
    );
    let tokens = shell_words::split(&sanitized).ok()?;
    classify_tokens(&tokens)
}

fn classify_tokens(tokens: &[String]) -> Option<PolicyGateAction> {
    for i in 0..tokens.len() {
        let token = &tokens[i];

        // Handle nested shell commands like `bash -c "git merge"`
        if token.contains(' ') || token.contains('\n') {
            if let Some(action) = classify_shell_command(token) {
                return Some(action);
            }
        }

        let program = program_basename(token);
        if let Some(action) = classify_command(&program, &tokens[i + 1..]) {
            return Some(action);
        }
    }
    None
}

fn strip_leading_tokens<'a>(tokens: &'a [String], value_flags: &[&str]) -> &'a [String] {
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if token == "--" {
            return &tokens[idx + 1..];
        }
        if is_assignment(token) {
            idx += 1;
            continue;
        }
        if !token.starts_with('-') {
            break;
        }

        let flag = token.split('=').next().unwrap_or(token);
        let takes_value = flag_is_value_flag(flag, value_flags);
        idx += 1;
        if takes_value && !token.contains('=') && idx < tokens.len() {
            idx += 1;
        }
    }
    &tokens[idx..]
}

fn flag_is_value_flag(flag: &str, value_flags: &[&str]) -> bool {
    value_flags.iter().any(|candidate| {
        *candidate == flag
            || (flag.starts_with(candidate) && flag != *candidate && candidate.starts_with('-'))
    })
}

fn is_assignment(token: &str) -> bool {
    token.contains('=') && !token.starts_with('-')
}

fn program_basename(token: &str) -> String {
    Path::new(token)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(token)
        .to_lowercase()
}

fn classify_command(program: &str, rest: &[String]) -> Option<PolicyGateAction> {
    match program {
        "git" => {
            let stripped = strip_leading_tokens(
                rest,
                &["-C", "-c", "--git-dir", "--work-tree", "--namespace"],
            );
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "commit"
                        | "log"
                        | "status"
                        | "diff"
                        | "show"
                        | "checkout"
                        | "branch"
                        | "fetch"
                        | "pull"
                        | "rebase"
                        | "reset"
                        | "revert"
                        | "clone"
                        | "init"
                        | "add"
                        | "rm"
                        | "mv"
                ) {
                    return None;
                }
                if t == "merge" {
                    return Some(PolicyGateAction::Merge);
                }
                if t == "push" {
                    return Some(PolicyGateAction::Deploy);
                }
            }
            for (i, t) in rest.iter().enumerate() {
                if t.starts_with("-c") || (i > 0 && rest[i - 1] == "-c") {
                    if t.contains("=merge") {
                        return Some(PolicyGateAction::Merge);
                    }
                    if t.contains("=push") {
                        return Some(PolicyGateAction::Deploy);
                    }
                }
            }
            None
        }
        "gh" => {
            let stripped =
                strip_leading_tokens(rest, &["-R", "--repo", "-H", "--hostname", "--workdir"]);
            let mut pr_seen = false;
            for t in stripped {
                if t == "pr" {
                    pr_seen = true;
                    continue;
                }
                if pr_seen && (t == "merge" || t.contains("=merge")) {
                    return Some(PolicyGateAction::Merge);
                }
            }
            None
        }
        "kubectl" => {
            let stripped = strip_leading_tokens(
                rest,
                &[
                    "-n",
                    "--namespace",
                    "--context",
                    "--kubeconfig",
                    "--cluster",
                    "--user",
                    "--server",
                    "--token",
                    "plugin",
                ],
            );
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "get"
                        | "describe"
                        | "logs"
                        | "explain"
                        | "exec"
                        | "port-forward"
                        | "proxy"
                        | "top"
                        | "auth"
                        | "create"
                        | "delete"
                        | "edit"
                        | "patch"
                        | "scale"
                        | "annotate"
                        | "label"
                ) {
                    return None;
                }
                if t == "apply" || t == "kustomize" {
                    return Some(PolicyGateAction::Apply);
                }
                if t == "rollout" {
                    return Some(PolicyGateAction::Deploy);
                }
            }
            None
        }
        "terraform" => {
            let stripped = strip_leading_tokens(rest, &["-chdir", "--chdir"]);
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "plan"
                        | "init"
                        | "validate"
                        | "fmt"
                        | "show"
                        | "state"
                        | "import"
                        | "refresh"
                        | "output"
                        | "providers"
                        | "graph"
                ) {
                    return None;
                }
                if t == "apply" {
                    return Some(PolicyGateAction::Apply);
                }
            }
            None
        }
        "helm" => {
            let stripped = strip_leading_tokens(rest, &["-n", "--namespace", "--kube-context"]);
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "install"
                        | "template"
                        | "show"
                        | "lint"
                        | "get"
                        | "history"
                        | "status"
                        | "list"
                        | "repo"
                        | "search"
                        | "env"
                        | "plugin"
                ) {
                    return None;
                }
                if t == "upgrade" {
                    return Some(PolicyGateAction::Deploy);
                }
            }
            None
        }
        "docker" => {
            let stripped = strip_leading_tokens(rest, &["-H", "--host"]);
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "build"
                        | "run"
                        | "pull"
                        | "images"
                        | "ps"
                        | "logs"
                        | "exec"
                        | "inspect"
                        | "stop"
                        | "rm"
                        | "rmi"
                        | "network"
                        | "volume"
                ) {
                    return None;
                }
                if t == "push" {
                    return Some(PolicyGateAction::Deploy);
                }
            }
            None
        }
        "cargo" => {
            let stripped = strip_leading_tokens(rest, &["-Z"]);
            for t in stripped {
                if matches!(
                    t.as_str(),
                    "build"
                        | "check"
                        | "test"
                        | "run"
                        | "fmt"
                        | "clippy"
                        | "doc"
                        | "update"
                        | "search"
                        | "install"
                        | "uninstall"
                ) {
                    return None;
                }
                if t == "publish" {
                    return Some(PolicyGateAction::Deploy);
                }
            }
            None
        }
        _ => None,
    }
}

pub fn evaluate(
    cfg: &PolicyGateConfig,
    action: PolicyGateAction,
    score: Option<f64>,
) -> PolicyGateDecision {
    if !is_action_gated(cfg, action) {
        return PolicyGateDecision::Allow;
    }

    match score {
        Some(s) => {
            if s < cfg.threshold {
                let msg = format!(
                    "Policy gate: aksi '{}' dibatasi karena validation score {:.2} < threshold {:.2}.",
                    action.as_str(),
                    s,
                    cfg.threshold
                );
                match cfg.mode {
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
            match cfg.mode {
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
    cfg.actions.contains(&action)
}

fn normalize_key(s: &str) -> String {
    s.trim().to_lowercase().replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_git_and_kubectl_variants() {
        assert_eq!(
            classify_shell_command("git merge main"),
            Some(PolicyGateAction::Merge)
        );
        assert_eq!(
            classify_shell_command("/usr/bin/git -C /repo push origin main"),
            Some(PolicyGateAction::Deploy)
        );
        assert_eq!(
            classify_shell_command("kubectl --context=prod apply -f k8s.yaml"),
            Some(PolicyGateAction::Apply)
        );
        assert_eq!(
            classify_shell_command("terraform -chdir=infra apply"),
            Some(PolicyGateAction::Apply)
        );
    }

    #[test]
    fn classifies_wrappers_and_nested_shells() {
        assert_eq!(
            classify_shell_command("sudo /usr/bin/git push origin main"),
            Some(PolicyGateAction::Deploy)
        );
        assert_eq!(
            classify_shell_command("env FOO=bar /usr/bin/gh pr merge 42"),
            Some(PolicyGateAction::Merge)
        );
        assert_eq!(
            classify_shell_command("bash -lc \"git merge origin/main\""),
            Some(PolicyGateAction::Merge)
        );
        assert_eq!(
            classify_shell_command("xargs -I{} git push origin main"),
            Some(PolicyGateAction::Deploy)
        );
    }

    #[test]
    fn classifies_more_deploy_paths() {
        assert_eq!(
            classify_shell_command("helm upgrade --install app chart"),
            Some(PolicyGateAction::Deploy)
        );
        assert_eq!(
            classify_shell_command("docker push ghcr.io/org/image:latest"),
            Some(PolicyGateAction::Deploy)
        );
        assert_eq!(
            classify_shell_command("cargo publish --dry-run"),
            Some(PolicyGateAction::Deploy)
        );
    }

    #[test]
    fn shell_parser_ignores_noise_but_not_actions() {
        assert_eq!(classify_shell_command(""), None);
        assert_eq!(classify_shell_command("echo merge"), None);
        assert_eq!(
            classify_shell_command("sudo -u root -E env FOO=bar git merge"),
            Some(PolicyGateAction::Merge)
        );
    }
}
