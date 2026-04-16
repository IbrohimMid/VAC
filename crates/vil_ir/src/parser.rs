//! Rust source file parser using `syn`.

use crate::error::{IrError, IrResult};
use crate::types::*;
use std::path::Path;
use quote::ToTokens;
use syn::{self, visit::Visit};

/// Parse a single Rust source file into IR.
pub fn parse_file(path: &Path) -> IrResult<IrModule> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| IrError::FileNotFound(path.display().to_string()))?;

    parse_source(&content, &path.display().to_string())
}

/// Parse Rust source code string into IR.
pub fn parse_source(source: &str, file_name: &str) -> IrResult<IrModule> {
    let syntax = syn::parse_file(source).map_err(|e| IrError::Parse {
        file: file_name.to_string(),
        message: e.to_string(),
    })?;

    let mut collector = IrCollector::new(file_name);
    collector.visit_file(&syntax);

    let mut module = collector.into_module();
    populate_line_spans(&mut module, source);
    Ok(module)
}

/// Post-process: populate line_span for functions and structs by searching source text.
fn populate_line_spans(module: &mut IrModule, source: &str) {
    let lines: Vec<&str> = source.lines().collect();

    for func in &mut module.functions {
        let pattern = format!("fn {}", func.name);
        for (i, line) in lines.iter().enumerate() {
            if line.contains(&pattern) {
                let start = i + 1; // 1-based
                // Find the matching closing brace by counting braces
                let mut depth = 0i32;
                let mut end = start;
                for (j, l) in lines.iter().enumerate().skip(i) {
                    depth += l.chars().filter(|c| *c == '{').count() as i32;
                    depth -= l.chars().filter(|c| *c == '}').count() as i32;
                    if depth <= 0 && j > i {
                        end = j + 1; // 1-based
                        break;
                    }
                }
                func.line_span = (start, end);
                break;
            }
        }
    }

    for s in &mut module.structs {
        let pattern = format!("struct {}", s.name);
        for (i, line) in lines.iter().enumerate() {
            if line.contains(&pattern) {
                s.line_span = (i + 1, i + 1); // 1-based, single line for struct declaration
                break;
            }
        }
    }
}

/// AST visitor that collects IR representations.
struct IrCollector {
    file_name: String,
    functions: Vec<IrFunction>,
    structs: Vec<IrStruct>,
    enums: Vec<IrEnum>,
    traits: Vec<IrTrait>,
    impls: Vec<IrImpl>,
    uses: Vec<IrUse>,
}

impl IrCollector {
    fn new(file_name: &str) -> Self {
        Self {
            file_name: file_name.to_string(),
            functions: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            traits: Vec::new(),
            impls: Vec::new(),
            uses: Vec::new(),
        }
    }

    fn into_module(self) -> IrModule {
        let name = Path::new(&self.file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        IrModule {
            path: self.file_name,
            name,
            functions: self.functions,
            structs: self.structs,
            enums: self.enums,
            traits: self.traits,
            impls: self.impls,
            uses: self.uses,
            submodules: Vec::new(),
            doc_comment: None,
        }
    }

    fn extract_visibility(vis: &syn::Visibility) -> Visibility {
        match vis {
            syn::Visibility::Public(_) => Visibility::Public,
            syn::Visibility::Restricted(r) => {
                let path = r
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                match path.as_str() {
                    "crate" => Visibility::Crate,
                    "super" => Visibility::Super,
                    _ => Visibility::Restricted(path),
                }
            }
            syn::Visibility::Inherited => Visibility::Private,
        }
    }

    fn extract_type(ty: &syn::Type) -> TypeRef {
        match ty {
            syn::Type::Path(tp) => {
                let name = tp
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                let generics = if let Some(last) = tp.path.segments.last() {
                    match &last.arguments {
                        syn::PathArguments::AngleBracketed(args) => args
                            .args
                            .iter()
                            .filter_map(|arg| {
                                if let syn::GenericArgument::Type(t) = arg {
                                    Some(Self::extract_type(t))
                                } else {
                                    None
                                }
                            })
                            .collect(),
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                };

                TypeRef {
                    is_option: name == "Option",
                    is_result: name == "Result",
                    name,
                    generics,
                    is_reference: false,
                    is_mutable: false,
                    lifetime: None,
                }
            }
            syn::Type::Reference(r) => {
                let mut inner = Self::extract_type(&r.elem);
                inner.is_reference = true;
                inner.is_mutable = r.mutability.is_some();
                inner.lifetime = r.lifetime.as_ref().map(|l| l.ident.to_string());
                inner
            }
            _ => TypeRef {
                name: quote::quote!(#ty).to_string(),
                generics: Vec::new(),
                is_reference: false,
                is_mutable: false,
                lifetime: None,
                is_option: false,
                is_result: false,
            },
        }
    }

    fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
        let docs: Vec<String> = attrs
            .iter()
            .filter_map(|attr| {
                if attr.path().is_ident("doc") {
                    if let syn::Meta::NameValue(nv) = &attr.meta {
                        if let syn::Expr::Lit(lit) = &nv.value {
                            if let syn::Lit::Str(s) = &lit.lit {
                                return Some(s.value().trim().to_string());
                            }
                        }
                    }
                }
                None
            })
            .collect();

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    /// Extract VIL-specific attributes from an attribute list.
    /// Returns a list of attribute names like ["vil_handler", "vil_state", "tracing::instrument"].
    fn extract_vil_attrs(attrs: &[syn::Attribute]) -> Vec<String> {
        const VIL_ATTRS: &[&str] = &[
            "vil_handler",
            "vil_endpoint",
            "vil_state",
            "vil_event",
            "vil_fault",
            "vil_decision",
            "process",
            "vil_app",
            "vil_service",
            "connector_fault",
            "connector_event",
            "connector_state",
        ];
        attrs
            .iter()
            .filter_map(|attr| {
                let name = attr
                    .path()
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                if VIL_ATTRS
                    .iter()
                    .any(|v| name == *v || name.starts_with(&format!("{v}::")))
                {
                    Some(name)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Extract call paths from a function body block.
    /// Looks for path expressions that look like function calls (e.g. `std::fs::read`, `reqwest::blocking::get`).
    fn extract_call_paths(block: &syn::Block) -> Vec<String> {
        use quote::ToTokens;
        let body_str = block.to_token_stream().to_string();

        // Known blocking/dangerous call path patterns
        const PATTERNS: &[&str] = &[
            "std :: fs ::",
            "std :: thread :: sleep",
            "reqwest :: blocking ::",
            "std :: io ::",
            "std :: net ::",
            "tokio :: fs ::",
            "tracing ::",
        ];

        let mut calls = Vec::new();
        for pattern in PATTERNS {
            if body_str.contains(pattern) {
                // Normalize spacing from token stream
                calls.push(pattern.replace(" :: ", "::").replace(" ", ""));
            }
        }
        calls.dedup();
        calls
    }

    fn extract_derives(attrs: &[syn::Attribute]) -> Vec<String> {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("derive"))
            .flat_map(|attr| {
                let mut derives = Vec::new();
                let _ = attr.parse_nested_meta(|meta| {
                    derives.push(
                        meta.path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::"),
                    );
                    Ok(())
                });
                derives
            })
            .collect()
    }
}

impl<'ast> Visit<'ast> for IrCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let func = IrFunction {
            name: node.sig.ident.to_string(),
            visibility: Self::extract_visibility(&node.vis),
            is_async: node.sig.asyncness.is_some(),
            is_unsafe: node.sig.unsafety.is_some(),
            is_const: node.sig.constness.is_some(),
            generics: Vec::new(),
            params: node
                .sig
                .inputs
                .iter()
                .map(|arg| match arg {
                    syn::FnArg::Receiver(r) => FnParam {
                        name: "self".to_string(),
                        ty: TypeRef {
                            name: "Self".to_string(),
                            generics: Vec::new(),
                            is_reference: r.reference.is_some(),
                            is_mutable: r.mutability.is_some(),
                            lifetime: r
                                .reference
                                .as_ref()
                                .and_then(|(_, lt)| lt.as_ref())
                                .map(|l| l.ident.to_string()),
                            is_option: false,
                            is_result: false,
                        },
                        is_self: true,
                        is_mutable: r.mutability.is_some(),
                        is_reference: r.reference.is_some(),
                        lifetime: None,
                    },
                    syn::FnArg::Typed(pat) => {
                        let name = quote::quote!(#pat.pat).to_string();
                        FnParam {
                            name,
                            ty: Self::extract_type(&pat.ty),
                            is_self: false,
                            is_mutable: false,
                            is_reference: false,
                            lifetime: None,
                        }
                    }
                })
                .collect(),
            return_type: match &node.sig.output {
                syn::ReturnType::Default => None,
                syn::ReturnType::Type(_, ty) => Some(Self::extract_type(ty)),
            },
            where_clauses: Vec::new(),
            body_summary: {
                let body_str = node.block.to_token_stream().to_string();
                if body_str.len() > 2 { Some(body_str) } else { None }
            },
            body_calls: Self::extract_call_paths(&node.block),
            doc_comment: Self::extract_doc_comment(&node.attrs),
            line_span: (0, 0), // Populated by post-processing below
            vil_attrs: Self::extract_vil_attrs(&node.attrs),
        };
        self.functions.push(func);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let s = IrStruct {
            name: node.ident.to_string(),
            visibility: Self::extract_visibility(&node.vis),
            generics: Vec::new(),
            fields: node
                .fields
                .iter()
                .map(|f| StructField {
                    name: f.ident.as_ref().map(|i| i.to_string()),
                    ty: Self::extract_type(&f.ty),
                    visibility: Self::extract_visibility(&f.vis),
                    doc_comment: Self::extract_doc_comment(&f.attrs),
                })
                .collect(),
            derives: Self::extract_derives(&node.attrs),
            doc_comment: Self::extract_doc_comment(&node.attrs),
            line_span: (0, 0), // Populated by post-processing below
            vil_attrs: Self::extract_vil_attrs(&node.attrs),
        };
        self.structs.push(s);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let e = IrEnum {
            name: node.ident.to_string(),
            visibility: Self::extract_visibility(&node.vis),
            generics: Vec::new(),
            variants: node
                .variants
                .iter()
                .map(|v| EnumVariant {
                    name: v.ident.to_string(),
                    fields: v
                        .fields
                        .iter()
                        .map(|f| StructField {
                            name: f.ident.as_ref().map(|i| i.to_string()),
                            ty: Self::extract_type(&f.ty),
                            visibility: Self::extract_visibility(&f.vis),
                            doc_comment: None,
                        })
                        .collect(),
                    discriminant: v
                        .discriminant
                        .as_ref()
                        .map(|(_, expr)| quote::quote!(#expr).to_string()),
                    doc_comment: Self::extract_doc_comment(&v.attrs),
                })
                .collect(),
            derives: Self::extract_derives(&node.attrs),
            doc_comment: Self::extract_doc_comment(&node.attrs),
            line_span: (0, 0),
        };
        self.enums.push(e);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let path = quote::quote!(#node.tree).to_string();
        self.uses.push(IrUse {
            path,
            alias: None,
            visibility: Self::extract_visibility(&node.vis),
        });
        syn::visit::visit_item_use(self, node);
    }
}
