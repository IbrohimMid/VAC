use std::collections::{HashMap, HashSet};

use crate::services::ask_user::{AskUserOption, AskUserQuestionKind};

/// State for the `ask_user` tool popup (structured operator prompt).
#[derive(Debug, Clone)]
pub struct AskUserState {
    pub question: Option<String>,
    pub options: Vec<AskUserOption>,
    pub selected: usize,
    pub input: String,
    pub tool_call_id: Option<String>,
    pub allow_free_text: bool,
    pub question_kind: AskUserQuestionKind,
    pub multi_selected: HashSet<usize>,
    pub metadata: HashMap<String, String>,
    pub filter: String,
    pub search_active: bool,
    pub scroll: usize,
}

impl Default for AskUserState {
    fn default() -> Self {
        Self {
            question: None,
            options: Vec::new(),
            selected: 0,
            input: String::new(),
            tool_call_id: None,
            allow_free_text: true,
            question_kind: AskUserQuestionKind::SingleSelect,
            multi_selected: HashSet::new(),
            metadata: HashMap::new(),
            filter: String::new(),
            search_active: false,
            scroll: 0,
        }
    }
}
