//! Code analysis: type resolution, borrow checking preview, complexity metrics.

use crate::types::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct TypeResolver {
    known_types: HashMap<String, TypeInfo>,
}

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub name: String,
    pub kind: TypeKind,
    pub implements_traits: Vec<String>,
    pub is_copy: bool,
    pub is_clone: bool,
    pub is_send: bool,
    pub is_sync: bool,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Struct,
    Enum,
    Trait,
    Primitive,
    Reference,
    Unknown,
}

impl TypeResolver {
    pub fn new() -> Self {
        let mut resolver = Self {
            known_types: HashMap::new(),
        };
        for prim in &[
            "bool", "char", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
            "i128", "isize", "f32", "f64", "str", "String", "()",
        ] {
            resolver.known_types.insert(
                prim.to_string(),
                TypeInfo {
                    name: prim.to_string(),
                    kind: TypeKind::Primitive,
                    implements_traits: vec![],
                    is_copy: !matches!(*prim, "String" | "str"),
                    is_clone: true,
                    is_send: true,
                    is_sync: true,
                },
            );
        }
        resolver
    }

    pub fn register_module(&mut self, module: &IrModule) {
        for s in &module.structs {
            self.known_types.insert(
                s.name.clone(),
                TypeInfo {
                    name: s.name.clone(),
                    kind: TypeKind::Struct,
                    implements_traits: vec![],
                    is_copy: s.derives.contains(&"Copy".to_string()),
                    is_clone: s.derives.contains(&"Clone".to_string()),
                    is_send: true,
                    is_sync: true,
                },
            );
        }
        for e in &module.enums {
            self.known_types.insert(
                e.name.clone(),
                TypeInfo {
                    name: e.name.clone(),
                    kind: TypeKind::Enum,
                    implements_traits: vec![],
                    is_copy: e.derives.contains(&"Copy".to_string()),
                    is_clone: e.derives.contains(&"Clone".to_string()),
                    is_send: true,
                    is_sync: true,
                },
            );
        }
    }

    pub fn resolve(&self, type_ref: &TypeRef) -> Option<&TypeInfo> {
        self.known_types.get(&type_ref.name)
    }

    pub fn implements_trait(&self, type_name: &str, trait_name: &str) -> bool {
        self.known_types
            .get(type_name)
            .map(|info| info.implements_traits.contains(&trait_name.to_string()))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct ComplexityMetrics {
    pub cyclomatic: usize,
    pub cognitive: usize,
    pub parameter_count: usize,
    pub nesting_depth: usize,
    pub line_count: usize,
}

pub fn analyze_function_complexity(func: &IrFunction) -> ComplexityMetrics {
    ComplexityMetrics {
        cyclomatic: 1,
        cognitive: 0,
        parameter_count: func.params.len(),
        nesting_depth: 0,
        line_count: func.line_span.1.saturating_sub(func.line_span.0),
    }
}
