//! IR type representations extracted from Rust source.

use serde::{Deserialize, Serialize};

/// A parsed Rust module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrModule {
    pub path: String,
    pub name: String,
    pub functions: Vec<IrFunction>,
    pub structs: Vec<IrStruct>,
    pub enums: Vec<IrEnum>,
    pub traits: Vec<IrTrait>,
    pub impls: Vec<IrImpl>,
    pub uses: Vec<IrUse>,
    pub submodules: Vec<String>,
    pub doc_comment: Option<String>,
}

/// A parsed function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: String,
    pub visibility: Visibility,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_const: bool,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_type: Option<TypeRef>,
    pub where_clauses: Vec<WhereClause>,
    pub body_summary: Option<String>,
    /// Call paths found in the function body (e.g. "std::fs::read", "reqwest::blocking::get").
    #[serde(default)]
    pub body_calls: Vec<String>,
    pub doc_comment: Option<String>,
    pub line_span: (usize, usize),
    /// VIL-specific attributes detected on this function (e.g. ["vil_handler", "vil_handler::shm"])
    #[serde(default)]
    pub vil_attrs: Vec<String>,
}

/// A parsed struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrStruct {
    pub name: String,
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<StructField>,
    pub derives: Vec<String>,
    pub doc_comment: Option<String>,
    pub line_span: (usize, usize),
    /// VIL semantic role macros detected (e.g. ["vil_state"], ["vil_event"])
    #[serde(default)]
    pub vil_attrs: Vec<String>,
}

/// A parsed enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrEnum {
    pub name: String,
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub derives: Vec<String>,
    pub doc_comment: Option<String>,
    pub line_span: (usize, usize),
}

/// A parsed trait definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrTrait {
    pub name: String,
    pub visibility: Visibility,
    pub generics: Vec<GenericParam>,
    pub methods: Vec<IrFunction>,
    pub supertraits: Vec<String>,
    pub doc_comment: Option<String>,
    pub line_span: (usize, usize),
}

/// A parsed impl block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrImpl {
    pub self_type: String,
    pub trait_name: Option<String>,
    pub generics: Vec<GenericParam>,
    pub methods: Vec<IrFunction>,
    pub line_span: (usize, usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrUse {
    pub path: String,
    pub alias: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Crate,
    Super,
    Private,
    Restricted(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub kind: GenericKind,
    pub bounds: Vec<String>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenericKind {
    Type,
    Lifetime,
    Const { ty: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FnParam {
    pub name: String,
    pub ty: TypeRef,
    pub is_self: bool,
    pub is_mutable: bool,
    pub is_reference: bool,
    pub lifetime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    pub name: String,
    pub generics: Vec<TypeRef>,
    pub is_reference: bool,
    pub is_mutable: bool,
    pub lifetime: Option<String>,
    pub is_option: bool,
    pub is_result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructField {
    pub name: Option<String>,
    pub ty: TypeRef,
    pub visibility: Visibility,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<StructField>,
    pub discriminant: Option<String>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhereClause {
    pub type_name: String,
    pub bounds: Vec<String>,
}
