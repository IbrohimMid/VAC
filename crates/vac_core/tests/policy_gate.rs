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
    assert_eq!(
        classify_shell_command("timeout 30s git push origin main"),
        Some(PolicyGateAction::Deploy)
    );
    assert_eq!(
        classify_shell_command("nice -n 10 gh pr merge 42"),
        Some(PolicyGateAction::Merge)
    );
    assert_eq!(
        classify_shell_command("find . -exec git merge origin/main \\;"),
        Some(PolicyGateAction::Merge)
    );
    assert_eq!(
        classify_shell_command("eval \"docker push ghcr.io/org/image:latest\""),
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
        (
            "timeout -k 5s 30s git push origin main",
            PolicyGateAction::Deploy,
        ),
        ("nohup git merge main", PolicyGateAction::Merge),
        ("stdbuf -oL gh pr merge 42", PolicyGateAction::Merge),
        (
            "watch -n 1 docker push ghcr.io/org/image:latest",
            PolicyGateAction::Deploy,
        ),
        (
            "find src -type f -exec cargo publish --dry-run {} \\;",
            PolicyGateAction::Deploy,
        ),
        (
            "eval \"timeout 10s git merge main\"",
            PolicyGateAction::Merge,
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
fn safe_commands_are_not_classified_as_gated() {
    let safe_commands = [
        "git status",
        "git log --oneline -10",
        "git diff HEAD~1",
        "git branch -a",
        "git fetch origin",
        "git checkout -b feature/new",
        "git stash",
        "git stash pop",
        "git rebase main",
        "git add .",
        "git commit -m 'fix: typo'",
        "git clone https://github.com/org/repo.git",
        "cargo check -p vac_cli",
        "cargo build --release",
        "cargo test -p vac_core",
        "cargo clippy --workspace",
        "cargo fmt --all -- --check",
        "cargo bench -p vil_ir",
        "ls -la",
        "cat README.md",
        "echo hello",
        "pwd",
        "whoami",
        "date",
        "wc -l src/main.rs",
        "head -20 Cargo.toml",
        "tail -f /var/log/syslog",
        "grep -r 'fn main' src/",
        "find . -name '*.rs' -type f",
        "mkdir -p target/debug",
        "cp src/main.rs src/main.rs.bak",
        "rm target/debug/build -rf",
        "docker build -t app:latest .",
        "docker run --rm app:latest",
        "docker ps -a",
        "docker logs container_id",
        "kubectl get pods -n default",
        "kubectl describe pod my-pod",
        "kubectl logs my-pod -f",
        "kubectl port-forward svc/api 8080:80",
        "terraform plan",
        "terraform init",
        "terraform validate",
        "terraform fmt",
        "helm list",
        "helm template chart/",
        "helm lint chart/",
        "npm install",
        "npm run build",
        "npm test",
        "python3 -m pytest tests/",
        "make build",
        "curl -s https://api.example.com/health",
    ];

    for cmd in safe_commands {
        assert_eq!(
            classify_shell_command(cmd),
            None,
            "safe command should NOT be classified as gated action: {cmd}"
        );
    }
}

#[test]
fn extended_bypass_corpus_deploy_actions() {
    let deploy_corpus: &[(&str, PolicyGateAction)] = &[
        // git push variants
        ("git push", PolicyGateAction::Deploy),
        ("git push origin main", PolicyGateAction::Deploy),
        ("git push --force origin main", PolicyGateAction::Deploy),
        ("git push -u origin feature/branch", PolicyGateAction::Deploy),
        ("git push --tags", PolicyGateAction::Deploy),
        ("sudo git push origin main", PolicyGateAction::Deploy),
        ("env GIT_SSH_COMMAND='ssh -i key' git push origin main", PolicyGateAction::Deploy),
        ("bash -c \"git push origin main\"", PolicyGateAction::Deploy),
        ("sh -c \"git push origin main\"", PolicyGateAction::Deploy),
        ("timeout 60 git push origin main", PolicyGateAction::Deploy),
        ("nohup git push origin main", PolicyGateAction::Deploy),
        // docker push variants
        ("docker push registry.io/app:v1", PolicyGateAction::Deploy),
        ("sudo docker push registry.io/app:v1", PolicyGateAction::Deploy),
        ("env DOCKER_HOST=tcp://0.0.0.0:2376 docker push img:latest", PolicyGateAction::Deploy),
        ("bash -c \"docker push img:latest\"", PolicyGateAction::Deploy),
        // cargo publish variants
        ("cargo publish", PolicyGateAction::Deploy),
        ("cargo publish --allow-dirty", PolicyGateAction::Deploy),
        ("sudo cargo publish", PolicyGateAction::Deploy),
        ("env CARGO_REGISTRY_TOKEN=xxx cargo publish", PolicyGateAction::Deploy),
        // helm upgrade variants
        ("helm upgrade app chart/", PolicyGateAction::Deploy),
        ("helm upgrade --install app chart/ --set image.tag=v2", PolicyGateAction::Deploy),
        ("sudo helm upgrade app chart/", PolicyGateAction::Deploy),
        ("env KUBECONFIG=/etc/k8s/config helm upgrade app chart/", PolicyGateAction::Deploy),
        // kubectl rollout variants
        ("kubectl rollout restart deployment/app", PolicyGateAction::Deploy),
        ("kubectl rollout status deployment/app", PolicyGateAction::Deploy),
        ("sudo kubectl rollout restart deployment/app", PolicyGateAction::Deploy),
        ("kubectl -n production rollout restart deployment/app", PolicyGateAction::Deploy),
    ];

    for (cmd, expected) in deploy_corpus {
        assert_eq!(
            classify_shell_command(cmd),
            Some(*expected),
            "deploy command should classify: {cmd}"
        );
    }
}

#[test]
fn extended_bypass_corpus_merge_actions() {
    let merge_corpus: &[(&str, PolicyGateAction)] = &[
        ("git merge main", PolicyGateAction::Merge),
        ("git merge --no-ff feature/branch", PolicyGateAction::Merge),
        ("git merge --squash feature/branch", PolicyGateAction::Merge),
        ("sudo git merge main", PolicyGateAction::Merge),
        ("env GIT_AUTHOR_NAME=bot git merge main", PolicyGateAction::Merge),
        ("bash -c \"git merge main\"", PolicyGateAction::Merge),
        ("sh -c \"git merge main\"", PolicyGateAction::Merge),
        ("timeout 30 git merge main", PolicyGateAction::Merge),
        ("nohup git merge main", PolicyGateAction::Merge),
        ("nice -n 5 git merge main", PolicyGateAction::Merge),
        ("gh pr merge 42", PolicyGateAction::Merge),
        ("gh pr merge 42 --squash", PolicyGateAction::Merge),
        ("gh pr merge 42 --rebase", PolicyGateAction::Merge),
        ("gh pr merge 42 --merge", PolicyGateAction::Merge),
        ("sudo gh pr merge 42", PolicyGateAction::Merge),
        ("bash -c \"gh pr merge 42\"", PolicyGateAction::Merge),
        ("env GITHUB_TOKEN=xxx gh pr merge 100", PolicyGateAction::Merge),
    ];

    for (cmd, expected) in merge_corpus {
        assert_eq!(
            classify_shell_command(cmd),
            Some(*expected),
            "merge command should classify: {cmd}"
        );
    }
}

#[test]
fn extended_bypass_corpus_apply_actions() {
    let apply_corpus: &[(&str, PolicyGateAction)] = &[
        ("kubectl apply -f manifest.yaml", PolicyGateAction::Apply),
        ("kubectl apply -f - < manifest.yaml", PolicyGateAction::Apply),
        ("kubectl apply -k overlays/prod", PolicyGateAction::Apply),
        ("sudo kubectl apply -f manifest.yaml", PolicyGateAction::Apply),
        ("bash -c \"kubectl apply -f manifest.yaml\"", PolicyGateAction::Apply),
        ("env KUBECONFIG=/path/to/config kubectl apply -f manifest.yaml", PolicyGateAction::Apply),
        ("kubectl --context=staging apply -f manifest.yaml", PolicyGateAction::Apply),
        ("kubectl -n production apply -f manifest.yaml", PolicyGateAction::Apply),
        ("terraform apply", PolicyGateAction::Apply),
        ("terraform apply -auto-approve", PolicyGateAction::Apply),
        ("terraform apply plan.out", PolicyGateAction::Apply),
        ("terraform -chdir=modules/vpc apply", PolicyGateAction::Apply),
        ("sudo terraform apply", PolicyGateAction::Apply),
        ("bash -c \"terraform apply -auto-approve\"", PolicyGateAction::Apply),
        ("env TF_VAR_region=us-east-1 terraform apply", PolicyGateAction::Apply),
        ("timeout 300 terraform apply", PolicyGateAction::Apply),
    ];

    for (cmd, expected) in apply_corpus {
        assert_eq!(
            classify_shell_command(cmd),
            Some(*expected),
            "apply command should classify: {cmd}"
        );
    }
}

#[test]
fn evaluate_edge_cases() {
    // Disabled gate always allows
    let cfg = PolicyGateConfig {
        enable: false,
        threshold: 0.9,
        mode: PolicyGateMode::Strict,
        actions: vec![PolicyGateAction::Merge],
    };
    assert_eq!(evaluate(&cfg, PolicyGateAction::Merge, Some(0.1)), PolicyGateDecision::Allow);

    // Empty actions list means all gated
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.8,
        mode: PolicyGateMode::Strict,
        actions: vec![],
    };
    assert!(matches!(evaluate(&cfg, PolicyGateAction::Deploy, Some(0.5)), PolicyGateDecision::Block(_)));
    assert!(matches!(evaluate(&cfg, PolicyGateAction::Apply, Some(0.5)), PolicyGateDecision::Block(_)));
    assert!(matches!(evaluate(&cfg, PolicyGateAction::Merge, Some(0.5)), PolicyGateDecision::Block(_)));

    // Above threshold always allows
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.8,
        mode: PolicyGateMode::Strict,
        actions: vec![PolicyGateAction::Deploy],
    };
    assert_eq!(evaluate(&cfg, PolicyGateAction::Deploy, Some(0.95)), PolicyGateDecision::Allow);

    // Exact threshold allows
    assert_eq!(evaluate(&cfg, PolicyGateAction::Deploy, Some(0.8)), PolicyGateDecision::Allow);

    // Ungated action always allows even with low score
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: PolicyGateMode::Strict,
        actions: vec![PolicyGateAction::Merge],
    };
    assert_eq!(evaluate(&cfg, PolicyGateAction::Deploy, Some(0.1)), PolicyGateDecision::Allow);
}

#[test]
fn policy_gate_mode_parse() {
    assert_eq!(PolicyGateMode::parse("strict"), Some(PolicyGateMode::Strict));
    assert_eq!(PolicyGateMode::parse("Strict"), Some(PolicyGateMode::Strict));
    assert_eq!(PolicyGateMode::parse("STRICT"), Some(PolicyGateMode::Strict));
    assert_eq!(PolicyGateMode::parse("soft"), Some(PolicyGateMode::Soft));
    assert_eq!(PolicyGateMode::parse("Soft"), Some(PolicyGateMode::Soft));
    assert_eq!(PolicyGateMode::parse("  strict  "), Some(PolicyGateMode::Strict));
    assert_eq!(PolicyGateMode::parse("unknown"), None);
    assert_eq!(PolicyGateMode::parse(""), None);
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
