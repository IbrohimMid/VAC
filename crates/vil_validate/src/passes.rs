//! VIL Semantic Validation passes.
//!
//! Pass order (per RULES.md):
//!   1. Semantic correctness (SemanticModel-based)
//!   2. Zero-copy legality
//!   3. Observability completeness
//!   4. VIL Way compliance (forbidden constructs)

use vil_ir::semantic::{BoundaryType, MessageRole, SemanticModel};
use vil_ir::types::{IrModule, TypeRef};

pub struct ValidationReport {
    pub score: f64,
    pub issues: Vec<String>,
}

/// Run all VIL semantic validation passes on a module.
/// Passes are ordered per RULES.md: semantic first, then zero-copy, then observability, then VIL-way.
pub fn run_all_passes(module: &IrModule) -> ValidationReport {
    let mut issues = Vec::new();
    let semantic = SemanticModel::from_module(module);

    let scores = vec![
        pass_semantic_correctness(&semantic, &mut issues),
        pass_zero_copy_legality(module, &semantic, &mut issues),
        pass_observability(&semantic, module, &mut issues),
        pass_vil_way_compliance(module, &mut issues),
        pass_tri_lane_consistency(module, &semantic, &mut issues),
        pass_generated_plumbing(module, &mut issues),
        pass_semantic_macro_coverage(&semantic, module, &mut issues),
    ];

    let score = scores.iter().sum::<f64>() / scores.len() as f64;
    ValidationReport { score, issues }
}

/// Pass 1: Semantic correctness via SemanticModel.
/// Checks handler boundary classification and message role consistency.
fn pass_semantic_correctness(model: &SemanticModel, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;

    for handler in &model.handlers {
        // Network boundary handlers must be zero-copy eligible (ShmSlice or Bytes)
        if handler.boundary == BoundaryType::Network && !handler.zero_copy_eligible {
            issues.push(format!(
                "Handler '{}' is on Network boundary but is not zero-copy eligible. \
                Use ShmSlice instead of String/Vec<u8> for body extraction.",
                handler.name
            ));
            score *= 0.75;
        }
    }

    for msg in &model.messages {
        // Generic role on a message that looks like it should be semantic
        if msg.role == MessageRole::Generic {
            let name_lower = msg.name.to_lowercase();
            let looks_semantic = name_lower.contains("event")
                || name_lower.contains("state")
                || name_lower.contains("fault")
                || name_lower.contains("decision")
                || name_lower.contains("command")
                || name_lower.contains("request")
                || name_lower.contains("response");
            if looks_semantic {
                issues.push(format!(
                    "Struct '{}' looks like a semantic message type but has no VIL role macro. \
                    Add #[vil_state], #[vil_event], #[vil_fault], or #[vil_decision].",
                    msg.name
                ));
                score *= 0.9;
            }
        }
    }

    score
}

/// Pass 2: Zero-copy legality.
/// Network-boundary handlers must not take String/Vec on the hot path.
fn pass_zero_copy_legality(
    module: &IrModule,
    model: &SemanticModel,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;

    // Build a set of network handler names for fast lookup
    let network_handlers: std::collections::HashSet<&str> = model
        .handlers
        .iter()
        .filter(|h| h.boundary == BoundaryType::Network)
        .map(|h| h.name.as_str())
        .collect();

    for func in &module.functions {
        if !network_handlers.contains(func.name.as_str()) {
            continue;
        }
        for param in &func.params {
            if param.ty.name == "String" || param.ty.name == "Vec" {
                issues.push(format!(
                    "Handler '{}' takes '{}' on a Network boundary — this copies data. \
                    Use ShmSlice or Bytes for zero-copy body extraction.",
                    func.name, param.ty.name
                ));
                score *= 0.85;
            }
        }
    }

    score
}

/// Pass 3: Observability completeness.
/// Network handlers should have observability (tracing instrument or RequestId).
fn pass_observability(
    model: &SemanticModel,
    module: &IrModule,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;

    let uses_tracing = module.uses.iter().any(|u| u.path.contains("tracing"));
    let uses_request_id = module
        .functions
        .iter()
        .any(|f| f.params.iter().any(|p| p.ty.name.contains("RequestId")));

    let network_handler_count = model
        .handlers
        .iter()
        .filter(|h| h.boundary == BoundaryType::Network)
        .count();

    if network_handler_count > 0 && !uses_tracing && !uses_request_id {
        issues.push(format!(
            "Module '{}' has {} network handler(s) but no tracing or RequestId usage. \
            VIL requires observability completeness on network paths.",
            module.name, network_handler_count
        ));
        score *= 0.85;
    }

    // Handlers without observability_present get a lighter penalty
    for handler in &model.handlers {
        if handler.boundary == BoundaryType::Network && !handler.observability_present {
            issues.push(format!(
                "Advisory: Handler '{}' has no #[tracing::instrument]. \
                Add for full VIL observability.",
                handler.name
            ));
            score *= 0.97;
        }
    }

    score
}

/// Pass 4: VIL Way compliance.
/// Flags forbidden generic Axum patterns in VIL handlers.
fn pass_vil_way_compliance(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;

    for func in &module.functions {
        if !func.is_async || !matches!(func.visibility, vil_ir::types::Visibility::Public) {
            continue;
        }

        for param in &func.params {
            if has_type_name(&param.ty, "Json") {
                issues.push(format!(
                    "Handler '{}' uses Json<T> — forbidden in VIL. Use ShmSlice instead.",
                    func.name
                ));
                score *= 0.8;
            }
            if has_type_name(&param.ty, "Extension") {
                issues.push(format!(
                    "Handler '{}' uses Extension<T> — forbidden in VIL. Use ServiceCtx instead.",
                    func.name
                ));
                score *= 0.8;
            }
        }

        if let Some(ret) = &func.return_type {
            if has_type_name(ret, "Json") {
                issues.push(format!(
                    "Handler '{}' returns Json<T>. Use VilResponse instead.",
                    func.name
                ));
                score *= 0.8;
            }
        }
    }

    score
}

fn has_type_name(ty: &TypeRef, target: &str) -> bool {
    if ty.name.contains(target) {
        return true;
    }
    ty.generics.iter().any(|g| has_type_name(g, target))
}

/// Pass 5: Tri-Lane consistency.
/// Checks that handlers routing to different lanes don't accidentally block the fast lane.
fn pass_tri_lane_consistency(
    module: &IrModule,
    _model: &SemanticModel,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;
    
    for func in &module.functions {
        if func.vil_attrs.iter().any(|a| a.contains("lane = \"fast\"") || a.contains("fast_lane")) {
            if let Some(body) = &func.body_summary {
                if body.contains("fs::") || body.contains("reqwest::") || body.contains(".await") {
                    if body.contains("std::fs") || body.contains("std::thread::sleep") {
                        issues.push(format!(
                            "Handler '{}' is marked for the Fast Lane but contains synchronous blocking calls. Use async I/O or the Compute Lane.",
                            func.name
                        ));
                        score *= 0.8;
                    }
                }
            }
        }
    }
    score
}

/// Pass 6: Generated Plumbing.
/// Checks that users aren't manually writing code that VIL macros generate (e.g. implementing VilMessage manually).
fn pass_generated_plumbing(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;
    
    for imp in &module.impls {
        if imp.trait_name.as_deref() == Some("VilMessage") || imp.trait_name.as_deref() == Some("VilState") {
            issues.push(format!(
                "Struct '{}' manually implements '{}'. VIL macros (#[vil_message], #[vil_state]) automatically generate this plumbing. Remove the manual impl.",
                imp.self_type,
                imp.trait_name.as_ref().unwrap()
            ));
            score *= 0.8;
        }
    }
    score
}

/// Pass 7: Semantic Macro Coverage.
/// Ensures structs have explicit VIL macros rather than relying solely on naming heuristics.
fn pass_semantic_macro_coverage(
    model: &SemanticModel,
    module: &IrModule,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;
    
    for msg in &model.messages {
        if msg.role != MessageRole::Generic {
            // Find the struct in the module to check its actual attributes
            if let Some(s) = module.structs.iter().find(|s| s.name == msg.name) {
                let has_vil_attr = s.vil_attrs.iter().any(|a| a.starts_with("vil_"));
                if !has_vil_attr {
                    issues.push(format!(
                        "Struct '{}' is inferred as '{:?}' by name, but lacks explicit semantic macros (e.g., #[vil_state], #[vil_event]). Explicit macros are required for VIL-native execution.",
                        s.name, msg.role
                    ));
                    score *= 0.9;
                }
            }
        }
    }
    score
}
