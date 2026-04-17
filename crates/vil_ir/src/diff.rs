use crate::types::{IrFunction, IrModule};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Cosmetic,
    Semantic,
}

#[derive(Debug, Clone)]
pub struct IrChange {
    pub entity_name: String,
    pub entity_type: String, // "Function", "Struct", "Enum", "Trait"
    pub kind: ChangeKind,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ModuleDiff {
    pub path: String,
    pub changes: Vec<IrChange>,
    pub overall_kind: ChangeKind,
    pub modifies_generated_region: bool,
    /// Function renames detected in this module (after rename-dedup).
    pub renames: Vec<FunctionRename>,
}

/// A function rename detected across an old → new module pair.
///
/// Produced when a function appears "deleted" in the old IR and a function
/// with an identical signature (parameter type vector + return type) appears
/// "added" in the new IR within the same module.
#[derive(Debug, Clone)]
pub struct FunctionRename {
    pub module_path: String,
    pub old_name: String,
    pub new_name: String,
    /// 1.0 = exact signature match. This unit performs only exact matching;
    /// the field is kept open for future fuzzy scoring.
    pub signature_match_score: f32,
}

/// Aggregated report over one or more module diffs.
///
/// Adds counts, rename detection, and human-readable summary lines on top of
/// the raw `ModuleDiff` list, for agent / UI consumption.
#[derive(Debug, Clone, Default)]
pub struct IrDiffReport {
    pub module_diffs: Vec<ModuleDiff>,
    pub cosmetic_count: usize,
    pub semantic_count: usize,
    pub renames: Vec<FunctionRename>,
    pub summary_lines: Vec<String>,
}

impl IrDiffReport {
    /// Build a report from a collection of module diffs. Counts and summary
    /// lines are computed from the diffs' post-rename-dedup change lists.
    pub fn from_module_diffs(module_diffs: Vec<ModuleDiff>) -> Self {
        let mut cosmetic_count = 0;
        let mut semantic_count = 0;
        let mut renames = Vec::new();
        let mut summary_lines = Vec::new();

        for md in &module_diffs {
            for rename in &md.renames {
                summary_lines.push(format!(
                    "[Semantic] renamed fn {} → {} ({})",
                    rename.old_name, rename.new_name, md.path
                ));
                renames.push(rename.clone());
                semantic_count += 1;
            }

            for change in &md.changes {
                match change.kind {
                    ChangeKind::Cosmetic => cosmetic_count += 1,
                    ChangeKind::Semantic => semantic_count += 1,
                }
                summary_lines.push(summary_line_for_change(&md.path, change));
            }
        }

        Self {
            module_diffs,
            cosmetic_count,
            semantic_count,
            renames,
            summary_lines,
        }
    }
}

/// Render a single-line summary for an IrChange with a `[Kind]` prefix.
fn summary_line_for_change(module_path: &str, change: &IrChange) -> String {
    let kind = match change.kind {
        ChangeKind::Cosmetic => "Cosmetic",
        ChangeKind::Semantic => "Semantic",
    };
    let desc_lower = change.description.to_lowercase();
    let verb = if desc_lower == "added" {
        format!("added {} {}", change.entity_type.to_lowercase(), change.entity_name)
    } else if desc_lower == "removed" {
        format!("removed {} {}", change.entity_type.to_lowercase(), change.entity_name)
    } else if desc_lower.contains("doc comment") {
        format!("doc comment updated on {}", change.entity_name)
    } else {
        format!(
            "{} {}: {}",
            change.entity_type.to_lowercase(),
            change.entity_name,
            change.description
        )
    };
    format!("[{kind}] {verb} ({module_path})")
}

/// Return true iff two functions have structurally identical signatures
/// (parameter type vectors and return-type display paths). Parameter names
/// and lifetimes are ignored — rename detection is about *shape*, not
/// superficial diffs.
fn signatures_match(a: &IrFunction, b: &IrFunction) -> bool {
    if a.params.len() != b.params.len() {
        return false;
    }
    for (ap, bp) in a.params.iter().zip(b.params.iter()) {
        if ap.is_self != bp.is_self {
            return false;
        }
        if ap.ty.display_path() != bp.ty.display_path() {
            return false;
        }
    }
    let ar = a.return_type.as_ref().map(|t| t.display_path());
    let br = b.return_type.as_ref().map(|t| t.display_path());
    ar == br
}

pub fn diff_modules(old: Option<&IrModule>, new: Option<&IrModule>) -> Option<ModuleDiff> {
    if old.is_none() && new.is_none() {
        return None;
    }

    let path = old
        .map(|m| m.path.clone())
        .unwrap_or_else(|| new.unwrap().path.clone());
    let mut changes = Vec::new();
    let mut modifies_generated_region = false;

    let old_funcs = old.map(|m| m.functions.clone()).unwrap_or_default();
    let new_funcs = new.map(|m| m.functions.clone()).unwrap_or_default();

    let old_structs = old.map(|m| m.structs.clone()).unwrap_or_default();
    let new_structs = new.map(|m| m.structs.clone()).unwrap_or_default();

    // ---- Functions ----
    // Split into shared-name (modify candidates) and old-only / new-only (rename / add / delete).
    let mut old_only: Vec<&IrFunction> = Vec::new();
    let mut new_only_idx: Vec<usize> = Vec::new();

    for nf in &new_funcs {
        if let Some(of) = old_funcs.iter().find(|f| f.name == nf.name) {
            let mut semantic_change = false;
            let mut desc = Vec::new();
            if of.params.len() != nf.params.len()
                || of.return_type.as_ref().map(|t| &t.name)
                    != nf.return_type.as_ref().map(|t| &t.name)
            {
                semantic_change = true;
                desc.push("Signature changed");
            }
            if of.vil_attrs != nf.vil_attrs {
                semantic_change = true;
                desc.push("VIL attributes changed");
            }
            if of.body_summary != nf.body_summary && !semantic_change {
                // Body-only changes affect IR; treat as Semantic for now.
                semantic_change = true;
                desc.push("Body changed");
            }

            if semantic_change {
                if !of.vil_attrs.is_empty() {
                    modifies_generated_region = true;
                }
                changes.push(IrChange {
                    entity_name: nf.name.clone(),
                    entity_type: "Function".to_string(),
                    kind: ChangeKind::Semantic,
                    description: desc.join(", "),
                });
            }
        }
    }

    // Collect old-only
    for of in &old_funcs {
        if !new_funcs.iter().any(|f| f.name == of.name) {
            old_only.push(of);
        }
    }
    // Collect new-only (by index so we can mark them matched)
    for (i, nf) in new_funcs.iter().enumerate() {
        if !old_funcs.iter().any(|f| f.name == nf.name) {
            new_only_idx.push(i);
        }
    }

    // Rename detection: pair old_only with new_only where signatures match exactly.
    let mut renames: Vec<FunctionRename> = Vec::new();
    let mut matched_new: Vec<bool> = vec![false; new_only_idx.len()];
    let mut matched_old: Vec<bool> = vec![false; old_only.len()];

    for (oi, of) in old_only.iter().enumerate() {
        for (ni_pos, &ni) in new_only_idx.iter().enumerate() {
            if matched_new[ni_pos] {
                continue;
            }
            let nf = &new_funcs[ni];
            if signatures_match(of, nf) {
                if !of.vil_attrs.is_empty() || !nf.vil_attrs.is_empty() {
                    modifies_generated_region = true;
                }
                renames.push(FunctionRename {
                    module_path: path.clone(),
                    old_name: of.name.clone(),
                    new_name: nf.name.clone(),
                    signature_match_score: 1.0,
                });
                matched_new[ni_pos] = true;
                matched_old[oi] = true;
                break;
            }
        }
    }

    // Remaining new_only → Added
    for (ni_pos, &ni) in new_only_idx.iter().enumerate() {
        if matched_new[ni_pos] {
            continue;
        }
        let nf = &new_funcs[ni];
        changes.push(IrChange {
            entity_name: nf.name.clone(),
            entity_type: "Function".to_string(),
            kind: ChangeKind::Semantic,
            description: "Added".to_string(),
        });
    }

    // Remaining old_only → Removed
    for (oi, of) in old_only.iter().enumerate() {
        if matched_old[oi] {
            continue;
        }
        if !of.vil_attrs.is_empty() {
            modifies_generated_region = true;
        }
        changes.push(IrChange {
            entity_name: of.name.clone(),
            entity_type: "Function".to_string(),
            kind: ChangeKind::Semantic,
            description: "Removed".to_string(),
        });
    }

    // ---- Structs ----
    for ns in &new_structs {
        if let Some(os) = old_structs.iter().find(|s| s.name == ns.name) {
            let mut semantic_change = false;
            let mut desc = Vec::new();
            if os.fields.len() != ns.fields.len() {
                semantic_change = true;
                desc.push("Fields changed");
            }
            if os.vil_attrs != ns.vil_attrs {
                semantic_change = true;
                desc.push("VIL attributes changed");
            }
            if os.derives != ns.derives {
                semantic_change = true;
                desc.push("Derives changed");
            }

            if semantic_change {
                if !os.vil_attrs.is_empty() {
                    modifies_generated_region = true;
                }
                changes.push(IrChange {
                    entity_name: ns.name.clone(),
                    entity_type: "Struct".to_string(),
                    kind: ChangeKind::Semantic,
                    description: desc.join(", "),
                });
            }
        } else {
            changes.push(IrChange {
                entity_name: ns.name.clone(),
                entity_type: "Struct".to_string(),
                kind: ChangeKind::Semantic,
                description: "Added".to_string(),
            });
        }
    }

    for os in &old_structs {
        if !new_structs.iter().any(|s| s.name == os.name) {
            if !os.vil_attrs.is_empty() {
                modifies_generated_region = true;
            }
            changes.push(IrChange {
                entity_name: os.name.clone(),
                entity_type: "Struct".to_string(),
                kind: ChangeKind::Semantic,
                description: "Removed".to_string(),
            });
        }
    }

    let overall_kind = if !renames.is_empty()
        || changes.iter().any(|c| c.kind == ChangeKind::Semantic)
    {
        ChangeKind::Semantic
    } else {
        ChangeKind::Cosmetic
    };

    Some(ModuleDiff {
        path,
        changes,
        overall_kind,
        modifies_generated_region,
        renames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FnParam, IrFunction, IrModule, TypeRef, Visibility};

    fn ty(name: &str) -> TypeRef {
        TypeRef {
            name: name.to_string(),
            generics: vec![],
            is_reference: false,
            is_mutable: false,
            lifetime: None,
            is_option: false,
            is_result: false,
        }
    }

    fn fn_with(name: &str, params: Vec<(&str, &str)>, ret: Option<&str>) -> IrFunction {
        IrFunction {
            name: name.to_string(),
            visibility: Visibility::Public,
            is_async: false,
            is_unsafe: false,
            is_const: false,
            generics: vec![],
            params: params
                .into_iter()
                .map(|(pn, pt)| FnParam {
                    name: pn.to_string(),
                    ty: ty(pt),
                    is_self: false,
                    is_mutable: false,
                    is_reference: false,
                    lifetime: None,
                })
                .collect(),
            return_type: ret.map(ty),
            where_clauses: vec![],
            body_summary: None,
            body_calls: vec![],
            doc_comment: None,
            line_span: (0, 0),
            vil_attrs: vec![],
        }
    }

    fn empty_module(path: &str) -> IrModule {
        IrModule {
            path: path.to_string(),
            name: "m".to_string(),
            functions: vec![],
            structs: vec![],
            enums: vec![],
            traits: vec![],
            impls: vec![],
            uses: vec![],
            submodules: vec![],
            doc_comment: None,
        }
    }

    #[test]
    fn rename_detected_for_matching_signatures() {
        let mut old = empty_module("my_module.rs");
        old.functions
            .push(fn_with("foo", vec![("x", "i32")], Some("bool")));

        let mut new = empty_module("my_module.rs");
        new.functions
            .push(fn_with("bar", vec![("x", "i32")], Some("bool")));

        let diff = diff_modules(Some(&old), Some(&new)).expect("diff");
        assert_eq!(diff.renames.len(), 1, "expected exactly one rename");
        assert_eq!(diff.renames[0].old_name, "foo");
        assert_eq!(diff.renames[0].new_name, "bar");
        assert!((diff.renames[0].signature_match_score - 1.0).abs() < f32::EPSILON);

        // No orphan Add/Delete for the renamed pair.
        let has_added = diff
            .changes
            .iter()
            .any(|c| c.entity_name == "bar" && c.description == "Added");
        let has_removed = diff
            .changes
            .iter()
            .any(|c| c.entity_name == "foo" && c.description == "Removed");
        assert!(!has_added, "renamed fn should not appear as Added");
        assert!(!has_removed, "renamed fn should not appear as Removed");
    }

    #[test]
    fn mismatched_signatures_not_renamed() {
        let mut old = empty_module("m.rs");
        old.functions
            .push(fn_with("foo", vec![("x", "i32")], Some("bool")));

        let mut new = empty_module("m.rs");
        new.functions
            .push(fn_with("bar", vec![("x", "u64")], Some("bool")));

        let diff = diff_modules(Some(&old), Some(&new)).expect("diff");
        assert_eq!(diff.renames.len(), 0, "signature mismatch must not rename");
        assert!(diff.changes.iter().any(|c| c.entity_name == "bar" && c.description == "Added"));
        assert!(diff.changes.iter().any(|c| c.entity_name == "foo" && c.description == "Removed"));
    }

    #[test]
    fn report_counts_and_summary_lines() {
        let mut old = empty_module("my_module.rs");
        old.functions
            .push(fn_with("foo", vec![("x", "i32")], Some("bool")));
        // Deleted fn with a unique signature that nothing in `new` matches.
        old.functions
            .push(fn_with("gone", vec![("s", "String")], Some("u64")));

        let mut new = empty_module("my_module.rs");
        new.functions
            .push(fn_with("bar", vec![("x", "i32")], Some("bool"))); // renamed foo
        // Added fn with a unique signature that nothing in `old` matches.
        new.functions
            .push(fn_with("fresh", vec![("id", "usize")], Some("()"))); // added

        let diff = diff_modules(Some(&old), Some(&new)).expect("diff");
        let report = IrDiffReport::from_module_diffs(vec![diff]);

        assert_eq!(report.renames.len(), 1);
        // 1 rename (semantic) + 1 Added (semantic) + 1 Removed (semantic) = 3 semantic
        assert_eq!(report.semantic_count, 3);
        assert_eq!(report.cosmetic_count, 0);

        assert!(
            report
                .summary_lines
                .iter()
                .any(|l| l.starts_with("[Semantic] renamed fn foo → bar")),
            "expected rename summary line, got: {:?}",
            report.summary_lines
        );
        assert!(
            report
                .summary_lines
                .iter()
                .any(|l| l.starts_with("[Semantic] added ") && l.contains("fresh")),
            "expected added summary line, got: {:?}",
            report.summary_lines
        );
        assert!(
            report
                .summary_lines
                .iter()
                .any(|l| l.starts_with("[Semantic] removed ") && l.contains("gone")),
            "expected removed summary line, got: {:?}",
            report.summary_lines
        );
    }

    #[test]
    fn empty_diff_yields_empty_report() {
        let old = empty_module("m.rs");
        let new = empty_module("m.rs");
        let diff = diff_modules(Some(&old), Some(&new)).expect("diff");
        let report = IrDiffReport::from_module_diffs(vec![diff]);
        assert_eq!(report.semantic_count, 0);
        assert_eq!(report.cosmetic_count, 0);
        assert_eq!(report.renames.len(), 0);
        assert!(report.summary_lines.is_empty());
    }

    #[test]
    fn rename_does_not_pair_across_used_new() {
        // Two old-only fns with same signature, one new-only with same signature:
        // only the first old should be paired; the second must fall through to Removed.
        let mut old = empty_module("m.rs");
        old.functions
            .push(fn_with("a", vec![("x", "i32")], Some("bool")));
        old.functions
            .push(fn_with("b", vec![("x", "i32")], Some("bool")));

        let mut new = empty_module("m.rs");
        new.functions
            .push(fn_with("c", vec![("x", "i32")], Some("bool")));

        let diff = diff_modules(Some(&old), Some(&new)).expect("diff");
        assert_eq!(diff.renames.len(), 1);
        // The unpaired old should appear as Removed.
        let removed: Vec<_> = diff
            .changes
            .iter()
            .filter(|c| c.description == "Removed")
            .map(|c| c.entity_name.clone())
            .collect();
        assert_eq!(removed.len(), 1);
        // No Added.
        assert!(diff.changes.iter().all(|c| c.description != "Added"));
    }
}
