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
    /// Parse mode string. Unknown values return `None` so callers can
    /// fail-closed (treat as Strict) instead of silently degrading.
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
    let tokens = shell_words::split(command).ok()?;
    classify_tokens(&tokens)
}

fn classify_tokens(tokens: &[String]) -> Option<PolicyGateAction> {
    let program_token = tokens.first()?;
    let program = program_basename(program_token);
    let rest = &tokens[1..];

    match program.as_str() {
        "sudo" => classify_tokens(strip_leading_tokens(
            rest,
            &[
                "-u",
                "-g",
                "-h",
                "--user",
                "--group",
                "--chdir",
                "--login-shell",
                "--shell",
                "--preserve-env",
                "--set-home",
            ],
        )),
        "env" => classify_tokens(strip_leading_tokens(rest, &["-u", "-C", "-S"])),
        "xargs" => classify_tokens(strip_leading_tokens(
            rest,
            &["-n", "-L", "-P", "-I", "-s", "-d"],
        )),
        "timeout" => classify_timeout(rest),
        "nohup" | "nice" | "stdbuf" | "watch" => {
            classify_tokens(strip_leading_tokens(rest, &["-n", "-o", "-e", "-i"]))
        }
        "eval" => classify_eval(rest),
        "command" => classify_tokens(rest),
        "bash" | "sh" | "zsh" => {
            if let Some(nested) = nested_shell_command(rest) {
                return classify_shell_command(nested);
            }
            classify_tokens(strip_leading_tokens(rest, &[]))
        }
        "find" => classify_find(rest),
        _ => classify_command(&program, rest),
    }
}

fn classify_timeout(rest: &[String]) -> Option<PolicyGateAction> {
    let rest = strip_leading_tokens(
        rest,
        &[
            "-k",
            "--kill-after",
            "--signal",
            "-s",
            "--foreground",
            "--preserve-status",
        ],
    );
    let rest = skip_duration_prefix(rest);
    classify_tokens(rest)
}

fn classify_eval(rest: &[String]) -> Option<PolicyGateAction> {
    let nested = rest.join(" ");
    if nested.trim().is_empty() {
        return None;
    }
    classify_shell_command(&nested)
}

fn classify_find(rest: &[String]) -> Option<PolicyGateAction> {
    let mut idx = 0usize;
    while idx < rest.len() {
        let token = rest[idx].as_str();
        if matches!(token, "-exec" | "-execdir" | "-ok" | "-okdir") {
            let mut end = idx + 1;
            while end < rest.len() {
                if matches!(rest[end].as_str(), ";" | "+" | r"\;") {
                    break;
                }
                end += 1;
            }
            if end > idx + 1 {
                return classify_tokens(&rest[idx + 1..end]);
            }
            return None;
        }
        idx += 1;
    }
    None
}

fn classify_command(program: &str, rest: &[String]) -> Option<PolicyGateAction> {
    match program {
        "git" => classify_git(strip_leading_tokens(
            rest,
            &["-C", "-c", "--git-dir", "--work-tree", "--namespace"],
        )),
        "gh" => classify_gh(strip_leading_tokens(
            rest,
            &["-R", "--repo", "-H", "--hostname", "--workdir"],
        )),
        "kubectl" => classify_kubectl(strip_leading_tokens(
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
            ],
        )),
        "terraform" => classify_terraform(strip_leading_tokens(rest, &["-chdir"])),
        "helm" => classify_helm(strip_leading_tokens(
            rest,
            &["-n", "--namespace", "--kube-context"],
        )),
        "docker" => classify_docker(strip_leading_tokens(rest, &["-H", "--host"])),
        "cargo" => classify_cargo(strip_leading_tokens(rest, &["-Z"])),
        _ => None,
    }
}

fn classify_git(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("merge") => Some(PolicyGateAction::Merge),
        Some("push") => Some(PolicyGateAction::Deploy),
        _ => None,
    }
}

fn classify_gh(rest: &[String]) -> Option<PolicyGateAction> {
    match (
        rest.first().map(|s| s.as_str()),
        rest.get(1).map(|s| s.as_str()),
    ) {
        (Some("pr"), Some("merge")) => Some(PolicyGateAction::Merge),
        _ => None,
    }
}

fn classify_kubectl(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("apply") => Some(PolicyGateAction::Apply),
        Some("rollout") => Some(PolicyGateAction::Deploy),
        _ => None,
    }
}

fn classify_terraform(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("apply") => Some(PolicyGateAction::Apply),
        _ => None,
    }
}

fn classify_helm(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("upgrade") => Some(PolicyGateAction::Deploy),
        _ => None,
    }
}

fn classify_docker(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("push") => Some(PolicyGateAction::Deploy),
        _ => None,
    }
}

fn classify_cargo(rest: &[String]) -> Option<PolicyGateAction> {
    match rest.first().map(|s| s.as_str()) {
        Some("publish") => Some(PolicyGateAction::Deploy),
        _ => None,
    }
}

fn nested_shell_command(tokens: &[String]) -> Option<&str> {
    let mut idx = 0usize;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if token == "--" {
            return None;
        }
        if shell_command_flag(token) {
            return tokens.get(idx + 1).map(|s| s.as_str());
        }
        if shell_flag_consumes_next_value(token) {
            idx += 2;
            continue;
        }
        if !token.starts_with('-') {
            return None;
        }
        idx += 1;
    }
    None
}

fn shell_command_flag(token: &str) -> bool {
    token.starts_with('-')
        && !token.starts_with("--")
        && token.trim_start_matches('-').contains('c')
}

fn shell_flag_consumes_next_value(token: &str) -> bool {
    token.starts_with('-')
        && !token.starts_with("--")
        && token.trim_start_matches('-').contains('o')
        && !token.trim_start_matches('-').contains('c')
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
        if takes_value && flag_consumes_next_value(flag, token, value_flags) && idx < tokens.len() {
            idx += 1;
        }
    }
    &tokens[idx..]
}

fn skip_duration_prefix(tokens: &[String]) -> &[String] {
    let Some(first) = tokens.first() else {
        return tokens;
    };
    if is_duration_token(first) && tokens.len() > 1 {
        &tokens[1..]
    } else {
        tokens
    }
}

fn is_duration_token(token: &str) -> bool {
    let token = token.trim_start_matches('+');
    if token.is_empty() {
        return false;
    }

    let (numeric, suffix) = token.split_at(
        token
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(token.len()),
    );
    if numeric.is_empty() {
        return false;
    }

    if suffix.is_empty() {
        return true;
    }

    matches!(suffix, "s" | "m" | "h" | "d" | "w" | "ms" | "us" | "µs")
}

fn flag_is_value_flag(flag: &str, value_flags: &[&str]) -> bool {
    value_flags.iter().any(|candidate| {
        *candidate == flag
            || (flag.starts_with(candidate) && flag != *candidate && candidate.starts_with('-'))
    })
}

fn flag_consumes_next_value(flag: &str, token: &str, value_flags: &[&str]) -> bool {
    value_flags
        .iter()
        .any(|candidate| *candidate == flag && token == *candidate)
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
