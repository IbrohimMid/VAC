//! VIL Semantic Validation passes.

use vil_ir::types::{IrModule, TypeRef};

pub struct ValidationReport {
    pub score: f64,
    pub issues: Vec<String>,
}

/// Run all VIL semantic validation passes on a module.
pub fn run_all_passes(module: &IrModule) -> ValidationReport {
    let mut issues = Vec::new();

    let scores = vec![
        pass_vil_way_compliance(module, &mut issues),
        pass_zero_copy_legality(module, &mut issues),
        pass_observability(module, &mut issues),
    ];

    let total: f64 = scores.iter().sum();
    let score = if scores.is_empty() {
        1.0
    } else {
        total / scores.len() as f64
    };

    ValidationReport { score, issues }
}

/// Pass 1: VIL Way compliance
/// Flags forbidden usage of generic Axum patterns in VIL handlers.
fn pass_vil_way_compliance(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;

    for func in &module.functions {
        // Only inspect likely handlers (public async functions)
        if !func.is_async || !matches!(func.visibility, vil_ir::types::Visibility::Public) {
            continue;
        }

        let mut has_json = false;
        let mut has_extension = false;

        for param in &func.params {
            if has_type_name(&param.ty, "Json") {
                has_json = true;
            }
            if has_type_name(&param.ty, "Extension") {
                has_extension = true;
            }
        }

        if has_json {
            issues.push(format!(
                "Function '{}' uses Json<T> which is forbidden in VIL. Use ShmSlice instead.",
                func.name
            ));
            score *= 0.8;
        }

        if has_extension {
            issues.push(format!("Function '{}' uses Extension<T> which is forbidden in VIL. Use ServiceCtx instead.", func.name));
            score *= 0.8;
        }

        if let Some(ret) = &func.return_type {
            if has_type_name(ret, "Json") {
                issues.push(format!(
                    "Function '{}' returns Json<T>. Use VilResponse instead.",
                    func.name
                ));
                score *= 0.8;
            }
        }
    }

    score
}

/// Pass 2: Zero-copy legality
/// Flags likely illegal heap usage on zero-copy paths.
fn pass_zero_copy_legality(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;
    for func in &module.functions {
        for param in &func.params {
            // Simplified check: String or Vec on a public async path might copy, though not strictly forbidden.
            // If it's a VIL handler, they should prefer ShmSlice.
            if func.is_async && matches!(func.visibility, vil_ir::types::Visibility::Public) {
                if param.ty.name == "String" || param.ty.name == "Vec" {
                    // Just an advisory warning
                    issues.push(format!("Advisory: Function '{}' takes '{}'. Consider zero-copy types like ShmSlice or Bytes if this is a data lane.", func.name, param.ty.name));
                    score *= 0.95;
                }
            }
        }
    }
    score
}

/// Pass 3: Observability
/// Advises on adding #[tracing::instrument] or using RequestId.
fn pass_observability(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;

    // In actual implementation, we'd check attributes, but vil_ir might not parse all attrs yet.
    // For now, we just give a small penalty if there are no tracing macros or RequestId usages in the module.
    let uses_tracing = module.uses.iter().any(|u| u.path.contains("tracing"));
    if !uses_tracing && !module.functions.is_empty() {
        issues.push(format!(
            "Module '{}' has no tracing imports. VIL requires observability completeness.",
            module.name
        ));
        score *= 0.9;
    }

    score
}

fn has_type_name(ty: &TypeRef, target: &str) -> bool {
    if ty.name.contains(target) {
        return true;
    }
    for generic in &ty.generics {
        if has_type_name(generic, target) {
            return true;
        }
    }
    false
}
