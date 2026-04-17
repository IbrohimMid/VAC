use vac_core::config::PolicyGateConfig;
use vac_core::policy_gate::{
    PolicyGateAction, PolicyGateDecision, classify_shell_command, evaluate,
};

#[test]
fn classify_git_merge() {
    assert_eq!(
        classify_shell_command("git merge main"),
        Some(PolicyGateAction::Merge)
    );
}

#[test]
fn classify_kubectl_apply() {
    assert_eq!(
        classify_shell_command("kubectl apply -f k8s.yaml"),
        Some(PolicyGateAction::Apply)
    );
}

#[test]
fn classify_git_push() {
    assert_eq!(
        classify_shell_command("git push origin main"),
        Some(PolicyGateAction::Deploy)
    );
}

#[test]
fn strict_blocks_below_threshold() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: "strict".to_string(),
        actions: vec!["merge".to_string()],
    };
    let d = evaluate(&cfg, PolicyGateAction::Merge, Some(0.75));
    assert!(matches!(d, PolicyGateDecision::Block(_)));
}

#[test]
fn soft_warns_below_threshold() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: "soft".to_string(),
        actions: vec!["merge".to_string()],
    };
    let d = evaluate(&cfg, PolicyGateAction::Merge, Some(0.75));
    assert!(matches!(d, PolicyGateDecision::Warn(_)));
}

#[test]
fn missing_score_falls_back_to_warn() {
    let cfg = PolicyGateConfig {
        enable: true,
        threshold: 0.9,
        mode: "strict".to_string(),
        actions: vec!["deploy".to_string()],
    };
    let d = evaluate(&cfg, PolicyGateAction::Deploy, None);
    assert!(matches!(d, PolicyGateDecision::Warn(_)));
}
