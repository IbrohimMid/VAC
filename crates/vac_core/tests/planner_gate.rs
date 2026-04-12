//! Tests for vil_swarm::semantic planner gate.

use vil_swarm::semantic::{evaluate_planner_output, PlannerGateResult, TaskSemanticKind};

#[test]
fn passed_plan_with_knowledge_refs() {
    let output = r#"```json
{
  "kind": "VxApp",
  "semantic_roles": ["vil_state"],
  "lanes": ["Data"],
  "zero_copy_expected": true,
  "generated_plumbing_expected": true,
  "forbidden_constructs": ["Json<T>"],
  "required_patterns": ["vx_app_handler"],
  "knowledge_refs": ["vx_app_handler"],
  "rationale": "Server handler task"
}
```"#;

    match evaluate_planner_output(output) {
        PlannerGateResult::Passed(plan) => {
            assert_eq!(plan.kind, TaskSemanticKind::VxApp);
            assert!(!plan.knowledge_refs.is_empty());
        }
        other => panic!("Expected Passed, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn knowledge_gate_fails_for_vil_task_without_refs() {
    let output = r#"```json
{
  "kind": "VxApp",
  "semantic_roles": [],
  "lanes": ["Data"],
  "zero_copy_expected": true,
  "generated_plumbing_expected": false,
  "forbidden_constructs": [],
  "required_patterns": [],
  "knowledge_refs": [],
  "rationale": "forgot to consult knowledge"
}
```"#;

    match evaluate_planner_output(output) {
        PlannerGateResult::KnowledgeGateFailed(plan) => {
            assert_eq!(plan.kind, TaskSemanticKind::VxApp);
            assert!(plan.knowledge_refs.is_empty());
        }
        other => panic!("Expected KnowledgeGateFailed, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn generic_rust_passes_without_knowledge_refs() {
    let output = r#"```json
{
  "kind": "GenericRust",
  "semantic_roles": ["generic"],
  "lanes": [],
  "zero_copy_expected": false,
  "generated_plumbing_expected": false,
  "forbidden_constructs": [],
  "required_patterns": [],
  "knowledge_refs": [],
  "rationale": "Not a VIL task"
}
```"#;

    match evaluate_planner_output(output) {
        PlannerGateResult::Passed(plan) => {
            assert_eq!(plan.kind, TaskSemanticKind::GenericRust);
        }
        other => panic!("Expected Passed for GenericRust, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn parse_failed_on_invalid_json() {
    let output = "I analyzed the task and think we should use ShmSlice but forgot to output JSON";
    match evaluate_planner_output(output) {
        PlannerGateResult::ParseFailed(_) => {}
        other => panic!("Expected ParseFailed, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn parse_failed_on_malformed_json() {
    let output = "```json\n{ invalid json }\n```";
    match evaluate_planner_output(output) {
        PlannerGateResult::ParseFailed(_) => {}
        other => panic!("Expected ParseFailed, got {:?}", std::mem::discriminant(&other)),
    }
}
