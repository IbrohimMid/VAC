use crate::types::IrModule;

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
}

pub fn diff_modules(old: Option<&IrModule>, new: Option<&IrModule>) -> Option<ModuleDiff> {
    if old.is_none() && new.is_none() {
        return None;
    }
    
    let path = old.map(|m| m.path.clone()).unwrap_or_else(|| new.unwrap().path.clone());
    let mut changes = Vec::new();
    let mut modifies_generated_region = false;

    let old_funcs = old.map(|m| m.functions.clone()).unwrap_or_default();
    let new_funcs = new.map(|m| m.functions.clone()).unwrap_or_default();
    
    let old_structs = old.map(|m| m.structs.clone()).unwrap_or_default();
    let new_structs = new.map(|m| m.structs.clone()).unwrap_or_default();

    // Check functions
    for nf in &new_funcs {
        if let Some(of) = old_funcs.iter().find(|f| f.name == nf.name) {
            let mut semantic_change = false;
            let mut desc = Vec::new();
            if of.params.len() != nf.params.len() || of.return_type.as_ref().map(|t| &t.name) != nf.return_type.as_ref().map(|t| &t.name) {
                semantic_change = true;
                desc.push("Signature changed");
            }
            if of.vil_attrs != nf.vil_attrs {
                semantic_change = true;
                desc.push("VIL attributes changed");
            }
            if of.body_summary != nf.body_summary && !semantic_change {
                // Just a body change could be cosmetic or semantic, we consider it semantic if it affects IR
                // For simplicity, let's treat any body change as Semantic for IR
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
        } else {
            changes.push(IrChange {
                entity_name: nf.name.clone(),
                entity_type: "Function".to_string(),
                kind: ChangeKind::Semantic,
                description: "Added".to_string(),
            });
        }
    }

    for of in &old_funcs {
        if !new_funcs.iter().any(|f| f.name == of.name) {
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
    }

    // Check structs
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

    let overall_kind = if changes.iter().any(|c| c.kind == ChangeKind::Semantic) {
        ChangeKind::Semantic
    } else {
        ChangeKind::Cosmetic
    };

    Some(ModuleDiff {
        path,
        changes,
        overall_kind,
        modifies_generated_region,
    })
}
