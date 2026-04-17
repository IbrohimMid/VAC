use vac_core::config::{PolicyGateConfig, VacConfig};
use vac_core::policy_gate::{
    PolicyGateAction, PolicyGateDecision, PolicyGateMode, classify_shell_command, evaluate,
};

#[test]
fn classify_git_merge() {
    assert_eq!(
        classify_shell_command("git merge main"),
        Some(PolicyGateAction::Merge)
    );
}

#[test]
fn classify_flags_and_wrappers() {
    assert_eq!(
        classify_shell_command("/usr/bin/git -C /repo push origin main"),
        Some(PolicyGateAction::Deploy)
    );
    assert_eq!(
        classify_shell_command("sudo env FOO=bar /usr/bin/gh pr merge 42"),
        Some(PolicyGateAction::Merge)
    );
    assert_eq!(
        classify_shell_command("kubectl --context=prod apply -f k8s.yaml"),
        Some(PolicyGateAction::Apply)
    );
    assert_eq!(
        classify_shell_command("terraform -chdir=infra apply"),
        Some(PolicyGateAction::Apply)
    );
    assert_eq!(
        classify_shell_command("bash -lc \"git merge origin/main\""),
        Some(PolicyGateAction::Merge)
    );
    assert_eq!(
        classify_shell_command("bash -o pipefail -c \"git merge origin/main\""),
        Some(PolicyGateAction::Merge)
    );
    assert_eq!(
        classify_shell_command("xargs -I{} git push origin main"),
        Some(PolicyGateAction::Deploy)
    );
}

#[test]
fn bypass_corpus_covers_wrapper_variants() {
    let corpus = [
        ("sudo git merge main", PolicyGateAction::Merge),
        ("sudo /usr/bin/git merge main", PolicyGateAction::Merge),
        ("env FOO=bar git merge main", PolicyGateAction::Merge),
        (
            "env FOO=bar /usr/bin/git merge main",
            PolicyGateAction::Merge,
        ),
        ("bash -c \"git merge main\"", PolicyGateAction::Merge),
        ("bash -lc \"git merge main\"", PolicyGateAction::Merge),
        (
            "bash -euxo pipefail -c \"git merge main\"",
            PolicyGateAction::Merge,
        ),
        ("sh -c \"git merge main\"", PolicyGateAction::Merge),
        ("xargs -I{} git merge main", PolicyGateAction::Merge),
        ("git -C /repo merge main", PolicyGateAction::Merge),
        ("git -c core.editor=vim merge main", PolicyGateAction::Merge),
        ("gh pr merge 42", PolicyGateAction::Merge),
        ("sudo gh pr merge 42", PolicyGateAction::Merge),
        ("/usr/bin/gh pr merge 42", PolicyGateAction::Merge),
        ("kubectl apply -f k8s.yaml", PolicyGateAction::Apply),
        (
            "kubectl --context=prod apply -f k8s.yaml",
            PolicyGateAction::Apply,
        ),
        (
            "sudo /usr/bin/kubectl -n prod apply -f k8s.yaml",
            PolicyGateAction::Apply,
        ),
        ("terraform apply", PolicyGateAction::Apply),
        ("terraform -chdir=infra apply", PolicyGateAction::Apply),
        ("sudo terraform -chdir infra apply", PolicyGateAction::Apply),
        ("helm upgrade --install app chart", PolicyGateAction::Deploy),
        (
            "sudo helm upgrade --install app chart",
            PolicyGateAction::Deploy,
        ),
        (
            "docker push ghcr.io/org/image:latest",
            PolicyGateAction::Deploy,
        ),
        (
            "sudo /usr/bin/docker push ghcr.io/org/image:latest",
            PolicyGateAction::Deploy,
        ),
        ("cargo publish --dry-run", PolicyGateAction::Deploy),
        ("sudo cargo publish --dry-run", PolicyGateAction::Deploy),
        (
            "env FOO=bar docker push ghcr.io/org/image:latest",
            PolicyGateAction::Deploy,
        ),
        (
            "env FOO=bar cargo publish --dry-run",
            PolicyGateAction::Deploy,
        ),
        (
            "bash -lc \"docker push ghcr.io/org/image:latest\"",
            PolicyGateAction::Deploy,
        ),
        (
            "bash -lc \"cargo publish --dry-run\"",
            PolicyGateAction::Deploy,
        ),
        (
            "xargs -n 1 docker push ghcr.io/org/image:latest",
            PolicyGateAction::Deploy,
        ),
        (
            "xargs -n1 cargo publish --dry-run",
            PolicyGateAction::Deploy,
        ),
    ];

    for (command, expected) in corpus {
        assert_eq!(
            classify_shell_command(command),
            Some(expected),
            "command should classify: {command}"
        );
    }
}

#[test]
fn strict_blocks_below_threshold() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: PolicyGateMode::Strict,
        actions: vec![PolicyGateAction::Merge],
    };
    let d = evaluate(&cfg, PolicyGateAction::Merge, Some(0.75));
    assert!(matches!(d, PolicyGateDecision::Block(_)));
}

#[test]
fn soft_warns_below_threshold() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: PolicyGateMode::Soft,
        actions: vec![PolicyGateAction::Merge],
    };
    let d = evaluate(&cfg, PolicyGateAction::Merge, Some(0.75));
    assert!(matches!(d, PolicyGateDecision::Warn(_)));
}

#[test]
fn missing_score_blocks_in_strict() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: PolicyGateMode::Strict,
        actions: vec![PolicyGateAction::Deploy],
    };
    let d = evaluate(&cfg, PolicyGateAction::Deploy, None);
    assert!(matches!(d, PolicyGateDecision::Block(_)));
}

#[test]
fn missing_score_warns_in_soft() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: PolicyGateMode::Soft,
        actions: vec![PolicyGateAction::Deploy],
    };
    let d = evaluate(&cfg, PolicyGateAction::Deploy, None);
    assert!(matches!(d, PolicyGateDecision::Warn(_)));
}

#[test]
fn config_rejects_unknown_variants() {
    let config = r#"
[policy_gate]
enable = true
threshold = 0.9
mode = "mystery"
actions = ["merge"]
"#;
    assert!(toml::from_str::<VacConfig>(config).is_err());

    let config = r#"
[policy_gate]
enable = true
threshold = 0.9
mode = "strict"
actions = ["mystery"]
"#;
    assert!(toml::from_str::<VacConfig>(config).is_err());
}
