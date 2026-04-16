//! Refactoring primitives: rename, extract, inline (type-aware).
//! Also provides deterministic VIL contract repair patterns.

use crate::semantic::{BoundaryType, SemanticModel};
use crate::types::IrModule;

#[derive(Debug, Clone)]
pub enum RefactorOp {
    Rename {
        old_name: String,
        new_name: String,
        scope: RefactorScope,
    },
    ExtractFunction {
        source_file: String,
        line_start: usize,
        line_end: usize,
        new_fn_name: String,
    },
    InlineFunction {
        fn_name: String,
        call_site_file: String,
        call_site_line: usize,
    },
}

#[derive(Debug, Clone)]
pub enum RefactorScope {
    File(String),
    Module(String),
    Workspace,
}

#[derive(Debug, Clone)]
pub struct RefactorResult {
    pub files_modified: Vec<FileEdit>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEdit {
    pub path: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextEdit {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub new_text: String,
}

pub fn apply_refactor(_modules: &[IrModule], _op: &RefactorOp) -> RefactorResult {
    RefactorResult {
        files_modified: vec![],
        warnings: vec!["Refactoring engine not yet implemented.".to_string()],
    }
}

// ── VIL Contract Repair Patterns ────────────────────────────────────────────

/// Result of a contract repair analysis on a single file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RepairPlan {
    pub path: String,
    pub repairs: Vec<RepairAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RepairAction {
    pub pattern: String,
    pub entity_name: String,
    pub severity: String,
    pub description: String,
    pub edit: TextEdit,
}

/// Pattern 1: Replace `String`/`Vec<u8>` params with `ShmSlice` on network handlers.
pub fn repair_zero_copy(module: &IrModule, source: &str) -> Vec<RepairAction> {
    let model = SemanticModel::from_module(module);
    let network_handlers: std::collections::HashSet<&str> = model
        .handlers
        .iter()
        .filter(|h| h.boundary == BoundaryType::Network)
        .map(|h| h.name.as_str())
        .collect();

    let mut repairs = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for func in &module.functions {
        if !network_handlers.contains(func.name.as_str()) {
            continue;
        }
        for param in &func.params {
            if param.is_self {
                continue;
            }
            let is_owned_bytes = param.ty.name == "String"
                || param.ty.name == "Vec"
                || contains_owned_bytes(&param.ty);

            if !is_owned_bytes {
                continue;
            }

            // Find the param in the source by scanning the function's line range
            let (fn_start, fn_end) = func.line_span;
            if fn_start == 0 && fn_end == 0 {
                // No line span info — produce a line-0 advisory edit
                repairs.push(RepairAction {
                    pattern: "zero_copy".to_string(),
                    entity_name: func.name.clone(),
                    severity: "high".to_string(),
                    description: format!(
                        "Handler '{}' param '{}' uses owned type '{}' on network boundary. Replace with ShmSlice<'_> or Bytes.",
                        func.name, param.name, param.ty.name
                    ),
                    edit: TextEdit {
                        line: 0,
                        col_start: 0,
                        col_end: 0,
                        new_text: format!("{}: ShmSlice<'_>", param.name),
                    },
                });
                continue;
            }

            // Search for the param type in the function signature lines
            for line_idx in fn_start.saturating_sub(1)..fn_end.min(lines.len()) {
                let line = lines[line_idx];
                let search = format!("{}: {}", param.name, param.ty.name);
                if let Some(col) = line.find(&search) {
                    let replacement = format!("{}: ShmSlice<'_>", param.name);
                    repairs.push(RepairAction {
                        pattern: "zero_copy".to_string(),
                        entity_name: func.name.clone(),
                        severity: "high".to_string(),
                        description: format!(
                            "Replace '{}' param type '{}' with ShmSlice<'_> for zero-copy body extraction.",
                            param.name, param.ty.name
                        ),
                        edit: TextEdit {
                            line: line_idx + 1, // 1-based
                            col_start: col,
                            col_end: col + search.len(),
                            new_text: replacement,
                        },
                    });
                    break;
                }
            }
        }
    }

    repairs
}

/// Pattern 2: Add `#[tracing::instrument]` to network handlers that lack it.
pub fn repair_observability(module: &IrModule, source: &str) -> Vec<RepairAction> {
    let model = SemanticModel::from_module(module);
    let lines: Vec<&str> = source.lines().collect();

    let mut repairs = Vec::new();

    for handler in &model.handlers {
        if handler.boundary != BoundaryType::Network || handler.observability_present {
            continue;
        }

        // Find the function in module
        if let Some(func) = module.functions.iter().find(|f| f.name == handler.name) {
            let (fn_start, _fn_end) = func.line_span;

            if fn_start == 0 {
                repairs.push(RepairAction {
                    pattern: "observability".to_string(),
                    entity_name: handler.name.clone(),
                    severity: "low".to_string(),
                    description: format!(
                        "Add #[tracing::instrument(skip_all)] above handler '{}'.",
                        handler.name
                    ),
                    edit: TextEdit {
                        line: 0,
                        col_start: 0,
                        col_end: 0,
                        new_text: "#[tracing::instrument(skip_all)]".to_string(),
                    },
                });
                continue;
            }

            // Insert attribute on the line before the function
            let insert_line = fn_start.saturating_sub(1);
            // Detect indentation from the fn line
            let indent = if insert_line < lines.len() {
                let fn_line = lines[insert_line];
                let trimmed = fn_line.trim_start();
                &fn_line[..fn_line.len() - trimmed.len()]
            } else {
                ""
            };

            repairs.push(RepairAction {
                pattern: "observability".to_string(),
                entity_name: handler.name.clone(),
                severity: "low".to_string(),
                description: format!(
                    "Add #[tracing::instrument(skip_all)] above handler '{}'.",
                    handler.name
                ),
                edit: TextEdit {
                    line: insert_line + 1, // 1-based, insert before fn
                    col_start: 0,
                    col_end: 0,
                    new_text: format!("{}#[tracing::instrument(skip_all)]\n", indent),
                },
            });
        }
    }

    repairs
}

/// Pattern 3: Add `#[vil_state]`/`#[vil_event]` to structs that look semantic but lack macros.
pub fn repair_semantic_macros(module: &IrModule, source: &str) -> Vec<RepairAction> {
    let model = SemanticModel::from_module(module);
    let lines: Vec<&str> = source.lines().collect();

    let mut repairs = Vec::new();

    for msg in &model.messages {
        use crate::semantic::MessageRole;
        // Only fix structs with a non-generic role that lack explicit VIL attrs
        if msg.role == MessageRole::Generic {
            continue;
        }

        let Some(s) = module.structs.iter().find(|s| s.name == msg.name) else {
            continue;
        };

        let has_vil_attr = s.vil_attrs.iter().any(|a| a.starts_with("vil_"));
        if has_vil_attr {
            continue;
        }

        let macro_name = match msg.role {
            MessageRole::State => "vil_state",
            MessageRole::Event => "vil_event",
            MessageRole::Fault => "vil_fault",
            MessageRole::Decision => "vil_decision",
            MessageRole::Generic => continue,
        };

        let (struct_start, _) = s.line_span;
        if struct_start == 0 {
            repairs.push(RepairAction {
                pattern: "semantic_macro".to_string(),
                entity_name: s.name.clone(),
                severity: "low".to_string(),
                description: format!(
                    "Add #[{}] to struct '{}' (inferred role: {:?}).",
                    macro_name, s.name, msg.role
                ),
                edit: TextEdit {
                    line: 0,
                    col_start: 0,
                    col_end: 0,
                    new_text: format!("#[{}]", macro_name),
                },
            });
            continue;
        }

        let insert_line = struct_start.saturating_sub(1);
        let indent = if insert_line < lines.len() {
            let sl = lines[insert_line];
            let trimmed = sl.trim_start();
            &sl[..sl.len() - trimmed.len()]
        } else {
            ""
        };

        repairs.push(RepairAction {
            pattern: "semantic_macro".to_string(),
            entity_name: s.name.clone(),
            severity: "low".to_string(),
            description: format!(
                "Add #[{}] to struct '{}' (inferred role: {:?}).",
                macro_name, s.name, msg.role
            ),
            edit: TextEdit {
                line: insert_line + 1,
                col_start: 0,
                col_end: 0,
                new_text: format!("{}#[{}]\n", indent, macro_name),
            },
        });
    }

    repairs
}

/// Run all 3 repair patterns on a module and its source.
pub fn generate_repair_plan(module: &IrModule, source: &str, file_path: &str) -> RepairPlan {
    let mut repairs = Vec::new();
    repairs.extend(repair_zero_copy(module, source));
    repairs.extend(repair_observability(module, source));
    repairs.extend(repair_semantic_macros(module, source));

    RepairPlan {
        path: file_path.to_string(),
        repairs,
    }
}

/// Check if a TypeRef contains owned byte types (String, Vec<u8>) in generics.
fn contains_owned_bytes(ty: &crate::types::TypeRef) -> bool {
    if ty.name == "String" || ty.name == "Vec" {
        return true;
    }
    ty.generics.iter().any(|g| contains_owned_bytes(g))
}
