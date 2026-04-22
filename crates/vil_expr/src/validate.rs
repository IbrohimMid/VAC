use crate::ast::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub line: usize,
    pub col: usize,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

pub struct SymbolTable {
    // Stub implementation
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {}
    }

    pub fn contains(&self, ident: &str) -> bool {
        // Placeholder stub: allow all except 'unknown_ident' for testing
        // TODO(PR-T12): Replace with actual symbol resolution logic
        ident != "unknown_ident"
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

pub fn validate(expr: &Expr, raw_input: &str, symbols: &SymbolTable) -> ValidationReport {
    let mut report = ValidationReport::default();

    // Naive legacy term check (to be replaced with actual canonical linter later)
    if raw_input.contains("v-cel") {
        report.issues.push(ValidationIssue {
            line: 1,
            col: 1, // naive placeholder
            severity: Severity::Error,
            message: "legacy alias 'v-cel' detected; use 'vil-expr' instead".to_string(),
        });
    }

    validate_expr(expr, symbols, &mut report);

    report
}

fn validate_expr(expr: &Expr, symbols: &SymbolTable, report: &mut ValidationReport) {
    match expr {
        Expr::Literal(_) => {}
        Expr::Ident(id) => {
            if id == "v_cel" || id == "v-cel" {
                report.issues.push(ValidationIssue {
                    line: 1,
                    col: 1,
                    severity: Severity::Error,
                    message: format!("Legacy term detected: {}", id),
                });
            } else if !symbols.contains(id) {
                report.issues.push(ValidationIssue {
                    line: 1,
                    col: 1,
                    severity: Severity::Error,
                    message: format!("Unknown identifier: {}", id),
                });
            }
        }
        Expr::FieldAccess(base, _) => {
            validate_expr(base, symbols, report);
        }
        Expr::Index(base, index) => {
            validate_expr(base, symbols, report);
            validate_expr(index, symbols, report);
        }
        Expr::Call(base, args) => {
            validate_expr(base, symbols, report);
            for arg in args {
                validate_expr(arg, symbols, report);
            }
        }
        Expr::BinOp(left, _, right) => {
            validate_expr(left, symbols, report);
            validate_expr(right, symbols, report);
        }
        Expr::UnaryOp(_, right) => {
            validate_expr(right, symbols, report);
        }
        Expr::Ternary(cond, true_branch, false_branch) => {
            validate_expr(cond, symbols, report);
            validate_expr(true_branch, symbols, report);
            validate_expr(false_branch, symbols, report);
        }
    }
}
