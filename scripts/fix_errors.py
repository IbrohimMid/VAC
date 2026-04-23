import re

fixes = [
    # session_snapshot.rs
    ("/workspace/crates/vac_tui_runtime/src/session_snapshot.rs", r'\.todos', '.transcript.todos'),
    ("/workspace/crates/vac_tui_runtime/src/session_snapshot.rs", r'\.operator\.current_model', '.operator_config.operator.current_model'),
    ("/workspace/crates/vac_tui_runtime/src/session_snapshot.rs", r'\.side_panel\.section_collapsed', '.layout.side_panel.section_collapsed'),

    # workbench/agents.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/agents.rs", r'\.runtime\b', '.execution.runtime'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/agents.rs", r'\.theme\b', '.core.theme'),

    # workbench/approvals.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\.approvals\.pending_approvals', '.execution.approvals.pending_approvals'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\.approvals\.approval_explanations', '.execution.approvals.approval_explanations'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\.theme\b', '.core.theme'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\|v\| v\.clone\(\)', '|v: &String| v.clone()'),

    # workbench/plan.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/plan.rs", r'\.theme\b', '.core.theme'),

    # workbench/review.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\.theme\b', '.core.theme'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\.review\b', '.workspace.review'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\|p\| p\.ends_with', '|p: &String| p.ends_with'),

    # workbench/runtime.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/runtime.rs", r'\.runtime\b', '.execution.runtime'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/runtime.rs", r'\.theme\b', '.core.theme'),

    # workbench/sessions.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/sessions.rs", r'\.sessions\b', '.session.sessions'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/sessions.rs", r'\.theme\b', '.core.theme'),

    # view/mod.rs
    ("/workspace/crates/vac_tui_runtime/src/view/mod.rs", r'\.changeset_store\b', '.workspace.changeset_store'),

    # contracts_test.rs
    ("/workspace/crates/vac_tui_runtime/src/contracts_test.rs", r'\.overlay_manager\b', '.layout.overlay_manager'),
]

for path, pattern, replacement in fixes:
    try:
        with open(path, 'r', encoding='utf-8') as f:
            content = f.read()
        new_content = re.sub(pattern, replacement, content)
        if new_content != content:
            with open(path, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"Fixed {path}: {pattern}")
    except Exception as e:
        print(f"Error on {path}: {e}")

