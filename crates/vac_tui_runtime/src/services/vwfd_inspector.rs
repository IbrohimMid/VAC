//! VWFD Inspector — read-only tree view over a `VwfdDocument`.
//!
//! PR-T11 body: provides `VwfdInspectorState` (doc + flat-tree selection)
//! plus pure functions for tree building and rendering. Input handlers and
//! action wiring arrive in a follow-up (PR-T11b).

use crate::services::theme::{StyleKey, Theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use vil_vwfd::{VwfdDocument, VwfdError};

/// State owned by the VWFD workbench tab. Default is an empty inspector
/// (no document loaded yet).
#[derive(Debug, Default, Clone)]
pub struct VwfdInspectorState {
    pub doc: Option<VwfdDocument>,
    pub selected_idx: usize,
    pub list_scroll: usize,
    pub detail_scroll: u16,
    pub last_error: Option<String>,
    pub source_path: Option<String>,
}

impl VwfdInspectorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse + validate a YAML document and load it into the inspector.
    /// On failure the previous document is preserved and `last_error` is set.
    pub fn load_yaml(&mut self, yaml: &str) -> Result<(), VwfdError> {
        match vil_vwfd::from_yaml(yaml) {
            Ok(doc) => {
                self.doc = Some(doc);
                self.selected_idx = 0;
                self.list_scroll = 0;
                self.detail_scroll = 0;
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                Err(err)
            }
        }
    }

    pub fn clear(&mut self) {
        self.doc = None;
        self.selected_idx = 0;
        self.list_scroll = 0;
        self.detail_scroll = 0;
        self.last_error = None;
        self.source_path = None;
    }

    /// Build a flat (pre-order) tree from the currently loaded document.
    pub fn flat_tree(&self) -> Vec<VwfdTreeNode> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        build_flat_tree(doc)
    }

    pub fn select_next(&mut self) {
        let len = self.flat_tree().len();
        if len == 0 {
            return;
        }
        if self.selected_idx + 1 < len {
            self.selected_idx += 1;
            self.detail_scroll = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected_idx == 0 {
            return;
        }
        self.selected_idx -= 1;
        self.detail_scroll = 0;
    }

    pub fn selected(&self) -> Option<VwfdTreeNode> {
        self.flat_tree().get(self.selected_idx).cloned()
    }
}

// ── Tree node model ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VwfdTreeNode {
    pub label: String,
    pub depth: u8,
    pub kind: VwfdNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VwfdNodeKind {
    SectionRoot,
    SectionWorkflows,
    SectionTriggers,
    SectionHandlers,
    Workflow(usize),
    Step { workflow: usize, step: usize },
    Trigger(usize),
    Handler(usize),
}

fn build_flat_tree(doc: &VwfdDocument) -> Vec<VwfdTreeNode> {
    let mut out: Vec<VwfdTreeNode> = Vec::new();

    out.push(VwfdTreeNode {
        label: format!(
            "{} ({:?})",
            doc.metadata.name.clone(),
            doc.kind.clone().normalize(),
        ),
        depth: 0,
        kind: VwfdNodeKind::SectionRoot,
    });

    // Workflows
    out.push(VwfdTreeNode {
        label: format!("workflows ({})", doc.spec.workflows.len()),
        depth: 1,
        kind: VwfdNodeKind::SectionWorkflows,
    });
    for (i, wf) in doc.spec.workflows.iter().enumerate() {
        out.push(VwfdTreeNode {
            label: format!("• {}  ({} steps)", wf.name, wf.steps.len()),
            depth: 2,
            kind: VwfdNodeKind::Workflow(i),
        });
        for (j, step) in wf.steps.iter().enumerate() {
            out.push(VwfdTreeNode {
                label: format!("{}  →  {}", step.id, step.handler),
                depth: 3,
                kind: VwfdNodeKind::Step {
                    workflow: i,
                    step: j,
                },
            });
        }
    }

    // Triggers
    out.push(VwfdTreeNode {
        label: format!("triggers ({})", doc.spec.triggers.len()),
        depth: 1,
        kind: VwfdNodeKind::SectionTriggers,
    });
    for (i, t) in doc.spec.triggers.iter().enumerate() {
        out.push(VwfdTreeNode {
            label: format!("• {}  [{:?}]  → {}", t.name, t.kind, t.workflow),
            depth: 2,
            kind: VwfdNodeKind::Trigger(i),
        });
    }

    // Handlers
    out.push(VwfdTreeNode {
        label: format!("handlers ({})", doc.spec.handlers.len()),
        depth: 1,
        kind: VwfdNodeKind::SectionHandlers,
    });
    for (i, h) in doc.spec.handlers.iter().enumerate() {
        out.push(VwfdTreeNode {
            label: format!("• {}  [{:?}]", h.name, h.execution),
            depth: 2,
            kind: VwfdNodeKind::Handler(i),
        });
    }

    out
}

// ── Rendering ────────────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, state: &VwfdInspectorState, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    render_tree(f, state, theme, chunks[0]);
    render_detail(f, state, theme, chunks[1]);
}

fn render_tree(f: &mut Frame, state: &VwfdInspectorState, theme: &Theme, area: Rect) {
    let tree = state.flat_tree();

    let title = match state.doc.as_ref() {
        Some(doc) => format!("VWFD: {}", doc.metadata.name),
        None => "VWFD".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        ))
        .border_style(theme.style(StyleKey::BorderNormal));

    if tree.is_empty() {
        let msg = match &state.last_error {
            Some(err) => vec![
                Line::from(Span::styled(
                    "Failed to load VWFD document",
                    theme.style(StyleKey::Error).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
                Line::from(Span::styled(err.clone(), theme.style(StyleKey::Muted))),
            ],
            None => vec![Line::from(Span::styled(
                "No VWFD document loaded.",
                theme.style(StyleKey::Muted),
            ))],
        };
        f.render_widget(Paragraph::new(msg).block(block).wrap(Wrap { trim: true }), area);
        return;
    }

    let items: Vec<ListItem> = tree
        .iter()
        .map(|node| {
            let indent = "  ".repeat(node.depth as usize);
            let style = match node.kind {
                VwfdNodeKind::SectionRoot => theme
                    .style(StyleKey::Warning)
                    .add_modifier(Modifier::BOLD),
                VwfdNodeKind::SectionWorkflows
                | VwfdNodeKind::SectionTriggers
                | VwfdNodeKind::SectionHandlers => theme
                    .style(StyleKey::Accent)
                    .add_modifier(Modifier::BOLD),
                _ => theme.style(StyleKey::Normal),
            };
            ListItem::new(Line::from(vec![
                Span::styled(indent, theme.style(StyleKey::Muted)),
                Span::styled(node.label.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        theme
            .style(StyleKey::OverlaySelected)
            .add_modifier(Modifier::BOLD),
    );

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_idx));
    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_detail(f: &mut Frame, state: &VwfdInspectorState, theme: &Theme, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            "Detail",
            theme.style(StyleKey::Muted).add_modifier(Modifier::BOLD),
        ))
        .border_style(theme.style(StyleKey::BorderNormal));

    let Some(doc) = state.doc.as_ref() else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "(no selection)",
                theme.style(StyleKey::Muted),
            )))
            .block(block),
            area,
        );
        return;
    };

    let lines = match state.selected().map(|n| n.kind) {
        Some(VwfdNodeKind::SectionRoot) => detail_root(doc, theme),
        Some(VwfdNodeKind::SectionWorkflows) => {
            detail_section_count("Workflows", doc.spec.workflows.len(), theme)
        }
        Some(VwfdNodeKind::Workflow(idx)) => detail_workflow(doc, idx, theme),
        Some(VwfdNodeKind::Step { workflow, step }) => {
            detail_step(doc, workflow, step, theme)
        }
        Some(VwfdNodeKind::SectionTriggers) => {
            detail_section_count("Triggers", doc.spec.triggers.len(), theme)
        }
        Some(VwfdNodeKind::Trigger(idx)) => detail_trigger(doc, idx, theme),
        Some(VwfdNodeKind::SectionHandlers) => {
            detail_section_count("Handlers", doc.spec.handlers.len(), theme)
        }
        Some(VwfdNodeKind::Handler(idx)) => detail_handler(doc, idx, theme),
        None => vec![Line::from(Span::styled(
            "(no selection)",
            theme.style(StyleKey::Muted),
        ))],
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((state.detail_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn kv<'a>(label: &'a str, value: String, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label}: "), theme.style(StyleKey::Muted)),
        Span::styled(value, theme.style(StyleKey::Normal)),
    ])
}

fn detail_root(doc: &VwfdDocument, theme: &Theme) -> Vec<Line<'static>> {
    vec![
        kv("name", doc.metadata.name.clone(), theme),
        kv("kind", format!("{:?}", doc.kind.clone().normalize()), theme),
        kv("apiVersion", doc.api_version.clone(), theme),
        kv(
            "namespace",
            doc.metadata
                .namespace
                .clone()
                .unwrap_or_else(|| "(default)".to_string()),
            theme,
        ),
        kv(
            "workflows",
            doc.spec.workflows.len().to_string(),
            theme,
        ),
        kv("triggers", doc.spec.triggers.len().to_string(), theme),
        kv("handlers", doc.spec.handlers.len().to_string(), theme),
    ]
}

fn detail_section_count(name: &str, count: usize, theme: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            name.to_string(),
            theme.style(StyleKey::Accent).add_modifier(Modifier::BOLD),
        )),
        kv("count", count.to_string(), theme),
    ]
}

fn detail_workflow(doc: &VwfdDocument, idx: usize, theme: &Theme) -> Vec<Line<'static>> {
    let Some(wf) = doc.spec.workflows.get(idx) else {
        return vec![Line::from(Span::styled(
            "workflow out of range".to_string(),
            theme.style(StyleKey::Error),
        ))];
    };
    let mut out = vec![
        Line::from(Span::styled(
            wf.name.clone(),
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        )),
        kv(
            "description",
            wf.description.clone().unwrap_or_default(),
            theme,
        ),
        kv("steps", wf.steps.len().to_string(), theme),
        Line::raw(""),
    ];
    for step in &wf.steps {
        out.push(Line::from(vec![
            Span::styled("  • ".to_string(), theme.style(StyleKey::Muted)),
            Span::styled(step.id.clone(), theme.style(StyleKey::Accent)),
            Span::styled("  →  ".to_string(), theme.style(StyleKey::Muted)),
            Span::styled(step.handler.clone(), theme.style(StyleKey::Normal)),
        ]));
    }
    out
}

fn detail_step(
    doc: &VwfdDocument,
    workflow: usize,
    step: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let Some(wf) = doc.spec.workflows.get(workflow) else {
        return vec![Line::from(Span::styled(
            "workflow out of range".to_string(),
            theme.style(StyleKey::Error),
        ))];
    };
    let Some(step) = wf.steps.get(step) else {
        return vec![Line::from(Span::styled(
            "step out of range".to_string(),
            theme.style(StyleKey::Error),
        ))];
    };
    vec![
        Line::from(Span::styled(
            step.id.clone(),
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        )),
        kv("handler", step.handler.clone(), theme),
        kv(
            "condition",
            step.condition.clone().unwrap_or_default(),
            theme,
        ),
        kv(
            "onError",
            step.on_error.clone().unwrap_or_default(),
            theme,
        ),
        kv("inputs", step.inputs.len().to_string(), theme),
        kv("outputs", step.outputs.len().to_string(), theme),
    ]
}

fn detail_trigger(doc: &VwfdDocument, idx: usize, theme: &Theme) -> Vec<Line<'static>> {
    let Some(t) = doc.spec.triggers.get(idx) else {
        return vec![Line::from(Span::styled(
            "trigger out of range".to_string(),
            theme.style(StyleKey::Error),
        ))];
    };
    vec![
        Line::from(Span::styled(
            t.name.clone(),
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        )),
        kv("kind", format!("{:?}", t.kind), theme),
        kv("workflow", t.workflow.clone(), theme),
        kv("configKeys", t.config.len().to_string(), theme),
    ]
}

fn detail_handler(doc: &VwfdDocument, idx: usize, theme: &Theme) -> Vec<Line<'static>> {
    let Some(h) = doc.spec.handlers.get(idx) else {
        return vec![Line::from(Span::styled(
            "handler out of range".to_string(),
            theme.style(StyleKey::Error),
        ))];
    };
    vec![
        Line::from(Span::styled(
            h.name.clone(),
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        )),
        kv("execution", format!("{:?}", h.execution), theme),
        kv("image", h.image.clone().unwrap_or_default(), theme),
        kv(
            "entrypoint",
            h.entrypoint.clone().unwrap_or_default(),
            theme,
        ),
        kv("configKeys", h.config.len().to_string(), theme),
    ]
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
apiVersion: vil.vastar.io/v1
kind: VilServer
metadata:
  name: sample-vil-server
  namespace: demo
spec:
  workflows:
    - name: ingest-pipeline
      description: Ingest then transform.
      steps:
        - id: fetch
          handler: http-fetch
        - id: transform
          handler: jq-transform
  triggers:
    - name: hourly
      kind: cron
      workflow: ingest-pipeline
      config:
        schedule: "0 * * * *"
  handlers:
    - name: http-fetch
      execution: native
    - name: jq-transform
      execution: wasm
      image: ghcr.io/vastar/jq-transform:1.0
"#;

    #[test]
    fn vwfd_tree_builds_from_fixture() {
        let mut state = VwfdInspectorState::new();
        state.load_yaml(SAMPLE_YAML).expect("valid VWFD fixture");

        let tree = state.flat_tree();
        assert!(!tree.is_empty(), "tree should be non-empty after load");

        // Expect: Root + SectionWorkflows + 1 workflow row + 2 step rows
        //         + SectionTriggers + 1 trigger
        //         + SectionHandlers + 2 handlers = 10 nodes
        assert_eq!(tree.len(), 10, "unexpected flat-tree size: {}", tree.len());

        // Root is first, then the workflows section header
        assert!(matches!(tree[0].kind, VwfdNodeKind::SectionRoot));
        assert!(matches!(tree[1].kind, VwfdNodeKind::SectionWorkflows));
        assert!(matches!(tree[2].kind, VwfdNodeKind::Workflow(0)));
        assert!(matches!(
            tree[3].kind,
            VwfdNodeKind::Step {
                workflow: 0,
                step: 0
            }
        ));
    }

    #[test]
    fn vwfd_select_navigates() {
        let mut state = VwfdInspectorState::new();
        state.load_yaml(SAMPLE_YAML).expect("valid VWFD fixture");

        let len = state.flat_tree().len();
        assert!(len >= 4);

        assert_eq!(state.selected_idx, 0);
        state.select_next();
        assert_eq!(state.selected_idx, 1);
        state.select_next();
        assert_eq!(state.selected_idx, 2);
        state.select_prev();
        assert_eq!(state.selected_idx, 1);

        // Saturates at the last index.
        for _ in 0..len + 5 {
            state.select_next();
        }
        assert_eq!(state.selected_idx, len - 1);

        // Saturates at 0 going backwards.
        for _ in 0..len + 5 {
            state.select_prev();
        }
        assert_eq!(state.selected_idx, 0);
    }

    #[test]
    fn vwfd_load_yaml_reports_errors() {
        let mut state = VwfdInspectorState::new();
        let err = state
            .load_yaml("apiVersion: unknown/v0\nkind: Pipeline\nmetadata: {name: x}\nspec: {}")
            .unwrap_err();
        // Error path preserves diagnostics in state.last_error
        assert!(state.last_error.is_some());
        // Document did not mutate
        assert!(state.doc.is_none());
        // err is Display-able
        let _ = err.to_string();
    }
}
