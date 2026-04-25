use vac_shell_plan::{PlanStatus, parse_plan_front_matter};

const VALID: &str = r#"---
status: active
objective: "Land slice 12"
steps:
  - "extract parser"
  - "wire host"
blocked_on:
  - "review"
---

body markdown here
"#;

#[test]
fn parse_plan_front_matter_valid() {
    let m = parse_plan_front_matter(VALID).unwrap();
    assert_eq!(m.status, PlanStatus::Active);
    assert_eq!(m.objective.as_deref(), Some("Land slice 12"));
    assert_eq!(m.steps, vec!["extract parser", "wire host"]);
    assert_eq!(m.blocked_on, vec!["review"]);
}

#[test]
fn parse_plan_front_matter_missing_fence_errors() {
    let err = parse_plan_front_matter("no front matter\nbody").unwrap_err();
    assert!(format!("{err}").contains("front-matter"));
}

#[test]
fn parse_plan_front_matter_unknown_status_errors() {
    let src = "---\nstatus: weirdstate\n---\n";
    let err = parse_plan_front_matter(src).unwrap_err();
    assert!(format!("{err}").contains("status"));
}

#[test]
fn parse_plan_front_matter_defaults_when_steps_missing() {
    let src = "---\nstatus: draft\n---\n";
    let m = parse_plan_front_matter(src).unwrap();
    assert_eq!(m.status, PlanStatus::Draft);
    assert!(m.steps.is_empty());
    assert!(m.blocked_on.is_empty());
    assert!(m.objective.is_none());
}
