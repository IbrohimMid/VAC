//! VWFD semantic diff — compares two [`VwfdDocument`]s and produces a
//! structured [`VwfdDiff`] describing added/removed/modified steps plus any
//! expression-level deltas (conditions, inputs, outputs, on_error).
//!
//! Steps live under `spec.workflows[].steps[]`; the diff flattens them and
//! keys by `<workflow>/<step_id>` so identical step ids across different
//! workflows don't collide.
//!
//! Scope: step-level and first-class expression fields. Deeper AST delta
//! (per-token `vil-expr` walk) is left to a follow-up once `vil_expr` grows
//! beyond the placeholder parser.

use std::collections::BTreeMap;

use vil_vwfd::VwfdDocument;
use vil_vwfd::schema::VwfdStep;

/// Top-level diff result between two VWFD documents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VwfdDiff {
    /// Qualified step keys (`<workflow>/<step_id>`) that are only in `new`.
    pub added_steps: Vec<String>,
    /// Qualified step keys (`<workflow>/<step_id>`) that are only in `old`.
    pub removed_steps: Vec<String>,
    pub modified_steps: Vec<ModifiedStep>,
    /// Every expression-level change across all steps, flattened for easy
    /// iteration by the review overlay.
    pub modified_expressions: Vec<ExpressionDelta>,
}

/// A step that exists in both documents but changed in a non-structural way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifiedStep {
    /// `<workflow>/<step_id>`
    pub id: String,
    pub handler_changed: Option<(String, String)>,
    pub on_error_changed: Option<(Option<String>, Option<String>)>,
    pub condition_changed: Option<(Option<String>, Option<String>)>,
    pub inputs_changed: bool,
    pub outputs_changed: bool,
}

/// A single field-level expression change attached to a qualified step id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionDelta {
    pub step_id: String,
    pub field: ExpressionField,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionField {
    Condition,
    OnError,
    Handler,
    Inputs,
    Outputs,
}

impl VwfdDiff {
    pub fn is_empty(&self) -> bool {
        self.added_steps.is_empty()
            && self.removed_steps.is_empty()
            && self.modified_steps.is_empty()
            && self.modified_expressions.is_empty()
    }
}

fn flatten_steps(doc: &VwfdDocument) -> BTreeMap<String, &VwfdStep> {
    let mut out: BTreeMap<String, &VwfdStep> = BTreeMap::new();
    for wf in &doc.spec.workflows {
        for step in &wf.steps {
            out.insert(format!("{}/{}", wf.name, step.id), step);
        }
    }
    out
}

/// Compute a semantic diff between two VWFD documents.
pub fn diff(old: &VwfdDocument, new: &VwfdDocument) -> VwfdDiff {
    let old_steps = flatten_steps(old);
    let new_steps = flatten_steps(new);

    let mut out = VwfdDiff::default();

    for id in new_steps.keys() {
        if !old_steps.contains_key(id) {
            out.added_steps.push(id.clone());
        }
    }
    for id in old_steps.keys() {
        if !new_steps.contains_key(id) {
            out.removed_steps.push(id.clone());
        }
    }

    for (id, new_step) in &new_steps {
        let Some(old_step) = old_steps.get(id) else {
            continue;
        };
        if let Some(m) = diff_step(id, old_step, new_step, &mut out.modified_expressions) {
            out.modified_steps.push(m);
        }
    }

    out
}

fn diff_step(
    id: &str,
    old: &VwfdStep,
    new: &VwfdStep,
    deltas: &mut Vec<ExpressionDelta>,
) -> Option<ModifiedStep> {
    let mut changed = false;
    let mut m = ModifiedStep {
        id: id.to_string(),
        handler_changed: None,
        on_error_changed: None,
        condition_changed: None,
        inputs_changed: false,
        outputs_changed: false,
    };

    if old.handler != new.handler {
        changed = true;
        m.handler_changed = Some((old.handler.clone(), new.handler.clone()));
        deltas.push(ExpressionDelta {
            step_id: id.to_string(),
            field: ExpressionField::Handler,
            before: Some(old.handler.clone()),
            after: Some(new.handler.clone()),
        });
    }

    if old.condition != new.condition {
        changed = true;
        m.condition_changed = Some((old.condition.clone(), new.condition.clone()));
        deltas.push(ExpressionDelta {
            step_id: id.to_string(),
            field: ExpressionField::Condition,
            before: old.condition.clone(),
            after: new.condition.clone(),
        });
    }

    if old.on_error != new.on_error {
        changed = true;
        m.on_error_changed = Some((old.on_error.clone(), new.on_error.clone()));
        deltas.push(ExpressionDelta {
            step_id: id.to_string(),
            field: ExpressionField::OnError,
            before: old.on_error.clone(),
            after: new.on_error.clone(),
        });
    }

    if old.inputs != new.inputs {
        changed = true;
        m.inputs_changed = true;
        deltas.push(ExpressionDelta {
            step_id: id.to_string(),
            field: ExpressionField::Inputs,
            before: Some(format_json_map(&old.inputs)),
            after: Some(format_json_map(&new.inputs)),
        });
    }

    if old.outputs != new.outputs {
        changed = true;
        m.outputs_changed = true;
        deltas.push(ExpressionDelta {
            step_id: id.to_string(),
            field: ExpressionField::Outputs,
            before: Some(format_json_map(&old.outputs)),
            after: Some(format_json_map(&new.outputs)),
        });
    }

    if changed { Some(m) } else { None }
}

fn format_json_map(map: &std::collections::HashMap<String, serde_json::Value>) -> String {
    let mut sorted: Vec<(&String, &serde_json::Value)> = map.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());
    let pairs: Vec<String> = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{{{}}}", pairs.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vil_vwfd::from_yaml;

    fn doc(steps_yaml: &str) -> VwfdDocument {
        let yaml = format!(
            r#"
apiVersion: vil.dev/v1
kind: Workflow
metadata:
  name: sample
spec:
  workflows:
    - name: wf1
      steps:
{steps_yaml}
  triggers:
    - name: onStart
      kind: manual
      workflow: wf1
  handlers:
    - name: h1
      execution: native
    - name: h2
      execution: native
"#,
        );
        from_yaml(&yaml).unwrap()
    }

    #[test]
    fn diff_added_step_reports_addition_only() {
        let old = doc("        - id: s1\n          handler: h1\n");
        let new = doc(
            "        - id: s1\n          handler: h1\n        - id: s2\n          handler: h2\n",
        );
        let d = diff(&old, &new);
        assert_eq!(d.added_steps, vec!["wf1/s2".to_string()]);
        assert!(d.removed_steps.is_empty());
        assert!(d.modified_steps.is_empty());
        assert!(d.modified_expressions.is_empty());
    }

    #[test]
    fn diff_removed_step_reports_removal_only() {
        let old = doc(
            "        - id: s1\n          handler: h1\n        - id: s2\n          handler: h2\n",
        );
        let new = doc("        - id: s1\n          handler: h1\n");
        let d = diff(&old, &new);
        assert_eq!(d.removed_steps, vec!["wf1/s2".to_string()]);
        assert!(d.added_steps.is_empty());
    }

    #[test]
    fn diff_handler_change_is_modified_step() {
        let old = doc("        - id: s1\n          handler: h1\n");
        let new = doc("        - id: s1\n          handler: h2\n");
        let d = diff(&old, &new);
        assert_eq!(d.modified_steps.len(), 1);
        let m = &d.modified_steps[0];
        assert_eq!(m.id, "wf1/s1");
        assert_eq!(
            m.handler_changed,
            Some(("h1".to_string(), "h2".to_string()))
        );
        assert_eq!(d.modified_expressions.len(), 1);
        assert_eq!(d.modified_expressions[0].field, ExpressionField::Handler);
    }

    #[test]
    fn diff_condition_expression_delta_is_captured() {
        let old =
            doc("        - id: s1\n          handler: h1\n          condition: 'a > 1'\n");
        let new =
            doc("        - id: s1\n          handler: h1\n          condition: 'a > 2'\n");
        let d = diff(&old, &new);
        assert_eq!(d.modified_steps.len(), 1);
        assert_eq!(d.modified_expressions.len(), 1);
        let delta = &d.modified_expressions[0];
        assert_eq!(delta.field, ExpressionField::Condition);
        assert_eq!(delta.before.as_deref(), Some("a > 1"));
        assert_eq!(delta.after.as_deref(), Some("a > 2"));
    }

    #[test]
    fn diff_identical_documents_is_empty() {
        let old = doc(
            "        - id: s1\n          handler: h1\n        - id: s2\n          handler: h2\n",
        );
        let new = old.clone();
        let d = diff(&old, &new);
        assert!(d.is_empty(), "{d:?}");
    }
}
