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

        // Message roles — prefer explicit vil_attrs, fall back to name heuristic
        for s in &module.structs {
            let role = if s.vil_attrs.iter().any(|a| a == "vil_state") {
                MessageRole::State
            } else if s.vil_attrs.iter().any(|a| a == "vil_event") {
                MessageRole::Event
            } else if s.vil_attrs.iter().any(|a| a == "vil_fault") {
                MessageRole::Fault
            } else if s.vil_attrs.iter().any(|a| a == "vil_decision") {
                MessageRole::Decision
            } else if s.name.contains("State") {
                MessageRole::State
            } else if s.name.contains("Event") {
                MessageRole::Event
            } else if s.name.contains("Fault") {
                MessageRole::Fault
            } else if s.name.contains("Decision") {
                MessageRole::Decision
            } else {
                MessageRole::Generic
            };

            messages.push(MessageEntity {
                name: s.name.clone(),
                role,
            });
        }

        // Handlers — prefer explicit vil_handler/vil_endpoint attrs, fall back to public async heuristic
        for f in &module.functions {
            let is_vil_handler = !f.vil_attrs.is_empty();
            let is_public_async = f.is_async && matches!(f.visibility, crate::types::Visibility::Public);

            if !is_vil_handler && !is_public_async {
                continue;
            }

            let zero_copy_eligible = f
                .params
                .iter()
                .any(|p| has_type_name(&p.ty, "ShmSlice") || has_type_name(&p.ty, "Bytes"));
            let is_network_handler = is_vil_handler
                || f.params.iter().any(|p| {
                    has_type_name(&p.ty, "Request") || has_type_name(&p.ty, "ShmSlice")
                });
            let observability_present = f.vil_attrs.iter().any(|a| a.contains("instrument"))
                || f.doc_comment.as_ref().map_or(false, |c| c.contains("instrument"));

            handlers.push(HandlerEntity {
                name: f.name.clone(),
                boundary: if is_network_handler {
                    BoundaryType::Network
                } else {
                    BoundaryType::IntraProcess
                },
                zero_copy_eligible,
                observability_present,
            });
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
