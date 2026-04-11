//! Validation passes — building toward 10-pass IR validation.

use vil_ir::types::IrModule;

/// Run all validation passes on a module. Returns score 0.0 - 1.0.
pub fn run_all_passes(module: &IrModule) -> f64 {
    let passes: Vec<(&str, f64)> = vec![
        ("syntax_valid", pass_syntax_valid(module)),
        ("has_doc_comments", pass_doc_comments(module)),
        ("error_handling", pass_error_handling(module)),
        ("naming_conventions", pass_naming_conventions(module)),
        ("visibility_hygiene", pass_visibility(module)),
    ];

    let total: f64 = passes.iter().map(|(_, s)| s).sum();
    total / passes.len() as f64
}

/// Pass 1: Syntax validity (always 1.0 if we parsed it).
fn pass_syntax_valid(_module: &IrModule) -> f64 {
    1.0
}

/// Pass 2: Documentation coverage.
fn pass_doc_comments(module: &IrModule) -> f64 {
    let total_public = module
        .functions
        .iter()
        .filter(|f| matches!(f.visibility, vil_ir::types::Visibility::Public))
        .count()
        + module
            .structs
            .iter()
            .filter(|s| matches!(s.visibility, vil_ir::types::Visibility::Public))
            .count();

    if total_public == 0 {
        return 1.0;
    }

    let documented = module
        .functions
        .iter()
        .filter(|f| {
            matches!(f.visibility, vil_ir::types::Visibility::Public) && f.doc_comment.is_some()
        })
        .count()
        + module
            .structs
            .iter()
            .filter(|s| {
                matches!(s.visibility, vil_ir::types::Visibility::Public) && s.doc_comment.is_some()
            })
            .count();

    documented as f64 / total_public as f64
}

/// Pass 3: Error handling — functions returning Result.
fn pass_error_handling(module: &IrModule) -> f64 {
    let fns_with_errors = module
        .functions
        .iter()
        .filter(|f| f.return_type.as_ref().is_some_and(|rt| rt.is_result))
        .count();

    let total_fns = module.functions.len();
    if total_fns == 0 {
        return 1.0;
    }

    // Score higher if more functions use Result for error handling
    let ratio = fns_with_errors as f64 / total_fns as f64;
    // At least 50% should use Result
    if ratio >= 0.5 { 1.0 } else { ratio * 2.0 }
}

/// Pass 4: Naming conventions (snake_case for functions, CamelCase for types).
fn pass_naming_conventions(module: &IrModule) -> f64 {
    let fn_names: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    let type_names: Vec<&str> = module
        .structs
        .iter()
        .map(|s| s.name.as_str())
        .chain(module.enums.iter().map(|e| e.name.as_str()))
        .collect();

    let fns_ok = fn_names.iter().filter(|n| is_snake_case(n)).count();
    let types_ok = type_names.iter().filter(|n| is_camel_case(n)).count();

    let total = fn_names.len() + type_names.len();
    if total == 0 {
        return 1.0;
    }

    (fns_ok + types_ok) as f64 / total as f64
}

/// Pass 5: Visibility hygiene — prefer private by default.
fn pass_visibility(module: &IrModule) -> f64 {
    let total = module.functions.len() + module.structs.len();
    if total == 0 {
        return 1.0;
    }

    let private_count = module
        .functions
        .iter()
        .filter(|f| matches!(f.visibility, vil_ir::types::Visibility::Private))
        .count()
        + module
            .structs
            .iter()
            .filter(|s| matches!(s.visibility, vil_ir::types::Visibility::Private))
            .count();

    // More private = better hygiene, aim for at least 40%
    let ratio = private_count as f64 / total as f64;
    (ratio * 2.5).min(1.0)
}

fn is_snake_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_lowercase() || c == '_' || c.is_numeric())
}

fn is_camel_case(s: &str) -> bool {
    s.starts_with(|c: char| c.is_uppercase()) && !s.contains('_')
}
