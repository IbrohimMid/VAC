//! VIL Semantic Validation passes.
//!
//! Pass order (per RULES.md):
//!   1. Semantic correctness (SemanticModel-based)
//!   2. Zero-copy legality (AST-level generic walking)
//!   3. Observability completeness
//!   4. VIL Way compliance (forbidden constructs)
//!   5. Tri-Lane consistency (body_calls analysis)
//!   6. Generated Plumbing (manual impl + duplicate macro detection)
//!   7. Semantic Macro Coverage

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

    let scores = [
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

/// Pass 2: Zero-copy legality (UPGRADED — walks generics).
/// Network-boundary handlers must not take owned-bytes types anywhere in their param tree.
/// Catches: `String`, `Vec<u8>`, `Result<String, E>`, `Option<Vec<u8>>`, etc.
fn pass_zero_copy_legality(
    module: &IrModule,
    model: &SemanticModel,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;

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
            if param.is_self {
                continue;
            }
            // Walk the full type tree for owned-bytes types (String, Vec<u8>)
            if param.ty.contains_owned_bytes() {
                let type_desc = param.ty.display_path();
                issues.push(format!(
                    "Handler '{}' param '{}' contains owned-bytes type '{}' on a Network boundary — this copies data. \
                    Use ShmSlice or Bytes for zero-copy body extraction.",
                    func.name, param.name, type_desc
                ));
                score *= 0.85;
            }
        }
        // Also check return type for zero-copy violations
        if let Some(ret) = &func.return_type {
            if ret.contains_owned_bytes() && !ret.is_result {
                let type_desc = ret.display_path();
                issues.push(format!(
                    "Handler '{}' returns owned-bytes type '{}' on Network boundary. \
                    Consider VilResponse for zero-copy response serialization.",
                    func.name, type_desc
                ));
                score *= 0.9;
            }
        }
    }

    score
}

/// Pass 3: Observability completeness.
/// Network handlers should have observability (tracing instrument or RequestId).
fn pass_observability(model: &SemanticModel, module: &IrModule, issues: &mut Vec<String>) -> f64 {
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

/// Pass 5: Tri-Lane consistency (UPGRADED — uses body_calls).
/// Checks that handlers routing to different lanes don't accidentally block the fast lane.
/// Uses parsed `body_calls` from the function body instead of substring matching.
fn pass_tri_lane_consistency(
    module: &IrModule,
    _model: &SemanticModel,
    issues: &mut Vec<String>,
) -> f64 {
    let mut score = 1.0;

    // Blocking call patterns that must not appear in fast-lane handlers
    const BLOCKING_CALLS: &[&str] = &[
        "std::fs::",
        "std::thread::sleep",
        "reqwest::blocking::",
        "std::net::",
        "std::io::",
    ];

    for func in &module.functions {
        let is_fast_lane = func
            .vil_attrs
            .iter()
            .any(|a| a.contains("lane = \"fast\"") || a.contains("fast_lane"));

        if !is_fast_lane {
            continue;
        }

        // Check body_calls (from AST-level extraction in parser)
        for call in &func.body_calls {
            if BLOCKING_CALLS.iter().any(|b| call.starts_with(b)) {
                issues.push(format!(
                    "Handler '{}' is marked for the Fast Lane but calls '{}' which is blocking. \
                    Use async I/O or move to the Compute Lane.",
                    func.name, call
                ));
                score *= 0.8;
            }
        }

        // Fallback: also check body_summary for patterns not caught by body_calls
        if let Some(body) = &func.body_summary {
            if body.contains("std :: thread :: sleep") && !func.body_calls.iter().any(|c| c.contains("thread::sleep")) {
                issues.push(format!(
                    "Handler '{}' uses std::thread::sleep in the Fast Lane. Use tokio::time::sleep instead.",
                    func.name
                ));
                score *= 0.8;
            }
        }
    }
    score
}

/// Pass 6: Generated Plumbing (UPGRADED — detects encode/decode methods + duplicate macros).
/// Checks that users aren't manually writing code that VIL macros generate.
fn pass_generated_plumbing(module: &IrModule, issues: &mut Vec<String>) -> f64 {
    let mut score = 1.0;

    for imp in &module.impls {
        let trait_name = match imp.trait_name.as_deref() {
            Some("VilMessage") | Some("VilState") | Some("VilEvent") => {
                imp.trait_name.as_ref().unwrap()
            }
            _ => continue,
        };

        // Check if the impl has encode/decode methods (indicates hand-written plumbing)
        let has_encode = imp.methods.iter().any(|m| m.name == "encode" || m.name == "decode");
        let severity = if has_encode { "high" } else { "medium" };

        issues.push(format!(
            "Struct '{}' manually implements '{}' ({} — {}). \
            VIL macros (#[vil_message], #[vil_state]) automatically generate this plumbing. Remove the manual impl.",
            imp.self_type,
            trait_name,
            if has_encode { "includes encode/decode methods" } else { "trait impl only" },
            severity
        ));
        score *= 0.8;
    }

    // Detect structs that have both a #[vil_state] attr AND a manual VilState impl (duplicate)
    for s in &module.structs {
        let has_vil_attr = s.vil_attrs.iter().any(|a| a == "vil_state" || a == "vil_event" || a == "vil_message");
        if !has_vil_attr {
            continue;
        }
        let has_manual_impl = module.impls.iter().any(|imp| {
            imp.self_type == s.name
                && imp
                    .trait_name
                    .as_deref()
                    .is_some_and(|t| t == "VilState" || t == "VilEvent" || t == "VilMessage")
        });
        if has_manual_impl {
            issues.push(format!(
                "Struct '{}' has BOTH a VIL macro attribute AND a manual trait impl. \
                This is a conflict — remove the manual impl and let the macro generate it.",
                s.name
            ));
            score *= 0.7;
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
