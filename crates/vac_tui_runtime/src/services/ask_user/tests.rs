use super::*;
use std::collections::HashMap;

#[test]
fn parse_args_with_kind_and_metadata() {
    let json = r#"{
        "question": "Pick a framework",
        "options": [
            {"id": "a", "label": "Actix", "metadata": {"tier": "1"}},
            {"id": "b", "label": "Axum", "description": "Tower-based"}
        ],
        "kind": "multi_select",
        "metadata": {"source": "planner"},
        "allow_free_text": false
    }"#;
    let args = parse_args(json).unwrap();
    assert_eq!(args.effective_kind(), AskUserQuestionKind::MultiSelect);
    assert_eq!(args.metadata.get("source").unwrap(), "planner");
    assert_eq!(args.options[0].metadata.get("tier").unwrap(), "1");
    assert!(args.options[1].metadata.is_empty());
}

#[test]
fn effective_kind_infers_from_options_and_free_text() {
    // No options → FreeText
    let args = AskUserArgs {
        question: "why?".into(),
        options: vec![],
        allow_free_text: false,
        kind: None,
        metadata: HashMap::new(),
    };
    assert_eq!(args.effective_kind(), AskUserQuestionKind::FreeText);

    // Options + free_text → Mixed
    let args = AskUserArgs {
        question: "why?".into(),
        options: vec![AskUserOption {
            id: "a".into(),
            label: "A".into(),
            description: None,
            metadata: HashMap::new(),
        }],
        allow_free_text: true,
        kind: None,
        metadata: HashMap::new(),
    };
    assert_eq!(args.effective_kind(), AskUserQuestionKind::Mixed);

    // Options only → SingleSelect
    let args = AskUserArgs {
        question: "why?".into(),
        options: vec![AskUserOption {
            id: "a".into(),
            label: "A".into(),
            description: None,
            metadata: HashMap::new(),
        }],
        allow_free_text: false,
        kind: None,
        metadata: HashMap::new(),
    };
    assert_eq!(args.effective_kind(), AskUserQuestionKind::SingleSelect);
}

#[test]
fn transcript_annotation_format() {
    let ann = transcript_annotation("Which DB?", "postgres");
    assert_eq!(ann, "[ask-user:Q] Which DB? → postgres");
}

#[test]
fn transcript_annotation_truncates_long_question() {
    let q = "x".repeat(200);
    let ann = transcript_annotation(&q, "yes");
    assert!(ann.starts_with("[ask-user:Q] "));
    assert!(ann.contains('…'));
    assert!(ann.len() < 200);
}

#[test]
fn metadata_round_trip_through_json() {
    let opt = AskUserOption {
        id: "test".into(),
        label: "Test".into(),
        description: Some("desc".into()),
        metadata: HashMap::from([("key".into(), "val".into())]),
    };
    let json = serde_json::to_string(&opt).unwrap();
    let parsed: AskUserOption = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.metadata.get("key").unwrap(), "val");
}

#[test]
fn filtered_option_indices_fuzzy_orders_best_match_first() {
    let options = vec![
        AskUserOption {
            id: "rust".into(),
            label: "Rust".into(),
            description: None,
            metadata: HashMap::new(),
        },
        AskUserOption {
            id: "ruby".into(),
            label: "Ruby".into(),
            description: None,
            metadata: HashMap::new(),
        },
        AskUserOption {
            id: "java".into(),
            label: "Java".into(),
            description: None,
            metadata: HashMap::new(),
        },
    ];
    let filtered = filtered_option_indices("rb", &options);
    assert_eq!(filtered.first().copied(), Some(1));
}

#[test]
fn render_respects_filter_and_cursor_highlight() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut state = AppState::default();
    state.ask_user.question = Some("Pick one".into());
    state.ask_user.options = vec![
        AskUserOption {
            id: "rust".into(),
            label: "Rust".into(),
            description: None,
            metadata: HashMap::new(),
        },
        AskUserOption {
            id: "ruby".into(),
            label: "Ruby".into(),
            description: None,
            metadata: HashMap::new(),
        },
    ];
    state.ask_user.question_kind = AskUserQuestionKind::SingleSelect;
    state.ask_user.selected = 0;
    state.ask_user.filter = "rb".into();
    state.ask_user.scroll = 0;
    crate::overlay::open_overlay(&mut state, crate::overlay::OverlayId::AskUser);
    let state = state;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ask_user_popup(f, &state)).unwrap();
    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    assert!(s.contains("Ruby"));
    assert!(!s.contains("Rust"));
}
