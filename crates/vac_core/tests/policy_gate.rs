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
        (r#"sudo git merge main"#, PolicyGateAction::Merge),
        (r#"sudo /usr/bin/git merge main"#, PolicyGateAction::Merge),
        (r#"env FOO=bar git merge main"#, PolicyGateAction::Merge),
        (
            r#"env FOO=bar /usr/bin/git merge main"#,
            PolicyGateAction::Merge,
        ),
        (r#"bash -c "git merge main""#, PolicyGateAction::Merge),
        (r#"bash -lc "git merge main""#, PolicyGateAction::Merge),
        (
            r#"bash -euxo pipefail -c "git merge main""#,
            PolicyGateAction::Merge,
        ),
        (r#"sh -c "git merge main""#, PolicyGateAction::Merge),
        (r#"xargs -I{} git merge main"#, PolicyGateAction::Merge),
        (r#"git -C /repo merge main"#, PolicyGateAction::Merge),
        (
            r#"git -c core.editor=vim merge main"#,
            PolicyGateAction::Merge,
        ),
        (r#"gh pr merge 42"#, PolicyGateAction::Merge),
        (r#"sudo gh pr merge 42"#, PolicyGateAction::Merge),
        (r#"/usr/bin/gh pr merge 42"#, PolicyGateAction::Merge),
        (r#"kubectl apply -f k8s.yaml"#, PolicyGateAction::Apply),
        (
            r#"kubectl --context=prod apply -f k8s.yaml"#,
            PolicyGateAction::Apply,
        ),
        (
            r#"sudo /usr/bin/kubectl -n prod apply -f k8s.yaml"#,
            PolicyGateAction::Apply,
        ),
        (r#"terraform apply"#, PolicyGateAction::Apply),
        (r#"terraform -chdir=infra apply"#, PolicyGateAction::Apply),
        (
            r#"sudo terraform -chdir infra apply"#,
            PolicyGateAction::Apply,
        ),
        (
            r#"helm upgrade --install app chart"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"sudo helm upgrade --install app chart"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"docker push ghcr.io/org/image:latest"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"sudo /usr/bin/docker push ghcr.io/org/image:latest"#,
            PolicyGateAction::Deploy,
        ),
        (r#"cargo publish --dry-run"#, PolicyGateAction::Deploy),
        (r#"sudo cargo publish --dry-run"#, PolicyGateAction::Deploy),
        (
            r#"env FOO=bar docker push ghcr.io/org/image:latest"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"env FOO=bar cargo publish --dry-run"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"bash -lc "docker push ghcr.io/org/image:latest""#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"bash -lc "cargo publish --dry-run""#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"xargs -n 1 docker push ghcr.io/org/image:latest"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"xargs -n1 cargo publish --dry-run"#,
            PolicyGateAction::Deploy,
        ),
        (r#"nohup git merge main"#, PolicyGateAction::Merge),
        (r#"nohup docker push img"#, PolicyGateAction::Deploy),
        (r#"nohup terraform apply"#, PolicyGateAction::Apply),
        (r#"timeout 10 git merge main"#, PolicyGateAction::Merge),
        (r#"timeout 10 docker push img"#, PolicyGateAction::Deploy),
        (r#"timeout 10 terraform apply"#, PolicyGateAction::Apply),
        (r#"stdbuf -oL git merge main"#, PolicyGateAction::Merge),
        (r#"stdbuf -oL docker push img"#, PolicyGateAction::Deploy),
        (r#"stdbuf -oL terraform apply"#, PolicyGateAction::Apply),
        (r#"watch -n 1 git merge main"#, PolicyGateAction::Merge),
        (r#"watch -n 1 docker push img"#, PolicyGateAction::Deploy),
        (r#"watch -n 1 terraform apply"#, PolicyGateAction::Apply),
        (r#"time git merge main"#, PolicyGateAction::Merge),
        (r#"time docker push img"#, PolicyGateAction::Deploy),
        (r#"time terraform apply"#, PolicyGateAction::Apply),
        (r#"nice -n 10 git merge main"#, PolicyGateAction::Merge),
        (r#"nice -n 10 docker push img"#, PolicyGateAction::Deploy),
        (r#"nice -n 10 terraform apply"#, PolicyGateAction::Apply),
        (r#"ionice -c 2 git merge main"#, PolicyGateAction::Merge),
        (r#"ionice -c 2 docker push img"#, PolicyGateAction::Deploy),
        (r#"ionice -c 2 terraform apply"#, PolicyGateAction::Apply),
        (r#"env -i git merge main"#, PolicyGateAction::Merge),
        (r#"env -i docker push img"#, PolicyGateAction::Deploy),
        (r#"env -i terraform apply"#, PolicyGateAction::Apply),
        (r#"sudo -E git merge main"#, PolicyGateAction::Merge),
        (r#"sudo -E docker push img"#, PolicyGateAction::Deploy),
        (r#"sudo -E terraform apply"#, PolicyGateAction::Apply),
        (r#"sudo -u root git merge main"#, PolicyGateAction::Merge),
        (r#"sudo -u root docker push img"#, PolicyGateAction::Deploy),
        (r#"sudo -u root terraform apply"#, PolicyGateAction::Apply),
        (r#"doas git merge main"#, PolicyGateAction::Merge),
        (r#"doas docker push img"#, PolicyGateAction::Deploy),
        (r#"doas terraform apply"#, PolicyGateAction::Apply),
        (r#"su -c git merge main"#, PolicyGateAction::Merge),
        (r#"su -c docker push img"#, PolicyGateAction::Deploy),
        (r#"su -c terraform apply"#, PolicyGateAction::Apply),
        (r#"eval 'git merge main'"#, PolicyGateAction::Merge),
        (r#"eval "git push""#, PolicyGateAction::Deploy),
        (r#"eval $(echo git merge main)"#, PolicyGateAction::Merge),
        (r#"eval `echo git push`"#, PolicyGateAction::Deploy),
        (r#"$(echo git) merge main"#, PolicyGateAction::Merge),
        (r#"git $(echo merge) main"#, PolicyGateAction::Merge),
        (r#"echo $(git merge main)"#, PolicyGateAction::Merge),
        (r#"$(git merge main)"#, PolicyGateAction::Merge),
        (r#"FOO=$(git merge main)"#, PolicyGateAction::Merge),
        (r#"FOO=$(git push) bar"#, PolicyGateAction::Deploy),
        (r#"$(which git) merge main"#, PolicyGateAction::Merge),
        (r#"echo $(terraform apply)"#, PolicyGateAction::Apply),
        (r#"`echo git` merge main"#, PolicyGateAction::Merge),
        (r#"git `echo merge` main"#, PolicyGateAction::Merge),
        (r#"echo `git merge main`"#, PolicyGateAction::Merge),
        (r#"`git merge main`"#, PolicyGateAction::Merge),
        (r#"FOO=`git push` bar"#, PolicyGateAction::Deploy),
        (r#"`which git` merge main"#, PolicyGateAction::Merge),
        (r#"echo `kubectl apply -f k.yaml`"#, PolicyGateAction::Apply),
        (
            r#"bash <<EOF
git merge main
EOF"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"sh -c 'git merge main' <<EOF
EOF"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"cat <<EOF | sh
git merge main
EOF"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"cat <<'EOF' | bash
git merge main
EOF"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"zsh <<EOF
git push
EOF"#,
            PolicyGateAction::Deploy,
        ),
        (r#"git merge main &|"#, PolicyGateAction::Merge),
        (r#"git merge main > /dev/null"#, PolicyGateAction::Merge),
        (r#"git merge main &> /dev/null"#, PolicyGateAction::Merge),
        (r#"env FOO=bar; git merge main"#, PolicyGateAction::Merge),
        (r#"git merge main; echo done"#, PolicyGateAction::Merge),
        (r#"git merge main && echo done"#, PolicyGateAction::Merge),
        (r#"echo start || git merge main"#, PolicyGateAction::Merge),
        (r#"git merge main | tee out.log"#, PolicyGateAction::Merge),
        (r#"{ git merge main; }"#, PolicyGateAction::Merge),
        (r#"(git merge main)"#, PolicyGateAction::Merge),
        (r#"echo main | xargs git merge"#, PolicyGateAction::Merge),
        (r#"echo main | xargs -t git merge"#, PolicyGateAction::Merge),
        (
            r#"echo main | xargs -I{} git merge {}"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"find . -name '*.txt' | xargs git push"#,
            PolicyGateAction::Deploy,
        ),
        (r#"xargs -0 git merge < file"#, PolicyGateAction::Merge),
        (r#"xargs --null git merge < file"#, PolicyGateAction::Merge),
        (r#"xargs -P 4 git push"#, PolicyGateAction::Deploy),
        (r#"find . -exec git merge main \;"#, PolicyGateAction::Merge),
        (
            r#"find . -exec git push origin main {} +"#,
            PolicyGateAction::Deploy,
        ),
        (
            r#"find . -type f -execdir git merge main \;"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"find . -name '*.yaml' -exec kubectl apply -f {} \;"#,
            PolicyGateAction::Apply,
        ),
        (r#"git -c alias.m=merge m main"#, PolicyGateAction::Merge),
        (r#"git --git-dir=.git merge main"#, PolicyGateAction::Merge),
        (r#"git --work-tree=. merge main"#, PolicyGateAction::Merge),
        (r#"git -C /tmp merge main"#, PolicyGateAction::Merge),
        (r#"git --namespace=foo merge main"#, PolicyGateAction::Merge),
        (
            r#"kubectl plugin apply -f foo.yaml"#,
            PolicyGateAction::Apply,
        ),
        (r#"kubectl kustomize apply"#, PolicyGateAction::Apply),
        (
            r#"kubectl --server=foo apply -f bar"#,
            PolicyGateAction::Apply,
        ),
        (
            r#"kubectl --token=foo apply -f bar"#,
            PolicyGateAction::Apply,
        ),
        (r#"terraform --chdir=infra apply"#, PolicyGateAction::Apply),
        (r#"g\i\t m\e\r\g\e main"#, PolicyGateAction::Merge),
        (r#""g"i"t" 'm'e'r'g'e' main"#, PolicyGateAction::Merge),
        (r#"git m"e"rge main"#, PolicyGateAction::Merge),
        (r#"git m\erge main"#, PolicyGateAction::Merge),
        (r#"g''i""t merge main"#, PolicyGateAction::Merge),
        (
            r#"sudo nohup timeout 10 git merge main"#,
            PolicyGateAction::Merge,
        ),
        (
            r#"env FOO=bar xargs -I{} sudo git merge {}"#,
            PolicyGateAction::Merge,
        ),
        (r#"bash -c "sudo git merge main""#, PolicyGateAction::Merge),
        (r#"sh -c 'nohup git merge main'"#, PolicyGateAction::Merge),
        (
            r#"eval "$(echo sudo git merge main)""#,
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

#[test]
fn test_policy_gate_action_as_str() {
    assert_eq!(
        vac_core::policy_gate::PolicyGateAction::Apply.as_str(),
        "apply"
    );
    assert_eq!(
        vac_core::policy_gate::PolicyGateAction::Deploy.as_str(),
        "deploy"
    );
    assert_eq!(
        vac_core::policy_gate::PolicyGateAction::Merge.as_str(),
        "merge"
    );
}

#[test]
fn test_policy_gate_mode_parse() {
    assert_eq!(
        vac_core::policy_gate::PolicyGateMode::parse("strict"),
        Some(vac_core::policy_gate::PolicyGateMode::Strict)
    );
    assert_eq!(
        vac_core::policy_gate::PolicyGateMode::parse("soft"),
        Some(vac_core::policy_gate::PolicyGateMode::Soft)
    );
    assert_eq!(
        vac_core::policy_gate::PolicyGateMode::parse("unknown"),
        None
    );
}

#[test]
fn test_evaluate_allow() {
    let cfg = vac_core::config::PolicyGateConfig {
        enable: true,
        threshold: 0.5,
        mode: vac_core::policy_gate::PolicyGateMode::Strict,
        actions: vec![vac_core::policy_gate::PolicyGateAction::Merge],
    };
    assert_eq!(
        vac_core::policy_gate::evaluate(
            &cfg,
            vac_core::policy_gate::PolicyGateAction::Merge,
            Some(0.9)
        ),
        vac_core::policy_gate::PolicyGateDecision::Allow
    );
    assert_eq!(
        vac_core::policy_gate::evaluate(
            &cfg,
            vac_core::policy_gate::PolicyGateAction::Deploy,
            None
        ),
        vac_core::policy_gate::PolicyGateDecision::Allow
    );
}

#[test]
fn test_evaluate_not_gated() {
    let cfg = vac_core::config::PolicyGateConfig {
        enable: false,
        threshold: 0.5,
        mode: vac_core::policy_gate::PolicyGateMode::Strict,
        actions: vec![vac_core::policy_gate::PolicyGateAction::Merge],
    };
    assert_eq!(
        vac_core::policy_gate::evaluate(
            &cfg,
            vac_core::policy_gate::PolicyGateAction::Merge,
            Some(0.1)
        ),
        vac_core::policy_gate::PolicyGateDecision::Allow
    );
}
