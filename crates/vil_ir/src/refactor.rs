//! Refactoring primitives: rename, extract, inline (type-aware).

use crate::types::IrModule;

#[derive(Debug, Clone)]
pub enum RefactorOp {
    Rename {
        old_name: String,
        new_name: String,
        scope: RefactorScope,
    },
    ExtractFunction {
        source_file: String,
        line_start: usize,
        line_end: usize,
        new_fn_name: String,
    },
    InlineFunction {
        fn_name: String,
        call_site_file: String,
        call_site_line: usize,
    },
}

#[derive(Debug, Clone)]
pub enum RefactorScope {
    File(String),
    Module(String),
    Workspace,
}

#[derive(Debug, Clone)]
pub struct RefactorResult {
    pub files_modified: Vec<FileEdit>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FileEdit {
    pub path: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone)]
pub struct TextEdit {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub new_text: String,
}

pub fn apply_refactor(_modules: &[IrModule], _op: &RefactorOp) -> RefactorResult {
    RefactorResult {
        files_modified: vec![],
        warnings: vec!["Refactoring engine not yet implemented.".to_string()],
    }
}
