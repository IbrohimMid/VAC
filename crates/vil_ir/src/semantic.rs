//! Semantic IR Adapter for VIL-native entities.

use crate::types::{IrModule, TypeRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticModel {
    pub module_name: String,
    pub handlers: Vec<HandlerEntity>,
    pub messages: Vec<MessageEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerEntity {
    pub name: String,
    pub boundary: BoundaryType,
    pub zero_copy_eligible: bool,
    pub observability_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntity {
    pub name: String,
    pub role: MessageRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BoundaryType {
    IntraProcess,
    InterProcess,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    State,
    Event,
    Fault,
    Decision,
    Generic,
}

impl SemanticModel {
    pub fn from_module(module: &IrModule) -> Self {
        let mut handlers = Vec::new();
        let mut messages = Vec::new();

        // Heuristics for Message roles
        for s in &module.structs {
            let role = if s
                .doc_comment
                .as_ref()
                .map_or(false, |c| c.contains("#[vil_state]"))
                || s.name.contains("State")
            {
                MessageRole::State
            } else if s
                .doc_comment
                .as_ref()
                .map_or(false, |c| c.contains("#[vil_event]"))
                || s.name.contains("Event")
            {
                MessageRole::Event
            } else if s
                .doc_comment
                .as_ref()
                .map_or(false, |c| c.contains("#[vil_fault]"))
                || s.name.contains("Fault")
            {
                MessageRole::Fault
            } else if s
                .doc_comment
                .as_ref()
                .map_or(false, |c| c.contains("#[vil_decision]"))
                || s.name.contains("Decision")
            {
                MessageRole::Decision
            } else {
                MessageRole::Generic
            };

            messages.push(MessageEntity {
                name: s.name.clone(),
                role,
            });
        }

        // Heuristics for Handlers
        for f in &module.functions {
            if f.is_async && matches!(f.visibility, crate::types::Visibility::Public) {
                let zero_copy_eligible = f
                    .params
                    .iter()
                    .any(|p| has_type_name(&p.ty, "ShmSlice") || has_type_name(&p.ty, "Bytes"));
                let is_network_handler = f
                    .params
                    .iter()
                    .any(|p| has_type_name(&p.ty, "Request") || has_type_name(&p.ty, "ShmSlice"));

                handlers.push(HandlerEntity {
                    name: f.name.clone(),
                    boundary: if is_network_handler {
                        BoundaryType::Network
                    } else {
                        BoundaryType::IntraProcess
                    },
                    zero_copy_eligible,
                    observability_present: f
                        .doc_comment
                        .as_ref()
                        .map_or(false, |c| c.contains("instrument")), // Approximation
                });
            }
        }

        Self {
            module_name: module.name.clone(),
            handlers,
            messages,
        }
    }
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
