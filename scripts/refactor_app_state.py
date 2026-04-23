import os
import re
import sys

field_map = {
    # Boot & Core (core)
    "hydrated": "core.hydrated",
    "hydration_deadline": "core.hydration_deadline",
    "loading": "core.loading",
    "loading_manager": "core.loading_manager",
    "view_flags": "core.view_flags",
    "quit": "core.quit",
    "input_tx": "core.input_tx",
    "project_root": "core.project_root",
    "theme": "core.theme",
    "render_metrics": "core.render_metrics",
    "startup": "core.startup",

    # UI & Layout (layout)
    "side_panel": "layout.side_panel",
    "focus": "layout.focus",
    "scroll": "layout.scroll",
    "workbench_tab": "layout.workbench_tab",
    "workbench_chrome": "layout.workbench_chrome",
    "command_palette": "layout.command_palette",
    "commands": "layout.commands",
    "banner": "layout.banner",
    "toasts": "layout.toasts",
    "image_render": "layout.image_render",
    "paste": "layout.paste",
    "message_ui": "layout.message_ui",
    "overlay_manager": "layout.overlay_manager",
    "lsp_ui": "layout.lsp_ui",
    "pins": "layout.pins",
    "switchers": "layout.switchers",
    "session_resume": "layout.session_resume",
    "ask_user": "layout.ask_user",

    # Input & Composer (composer)
    "input": "composer.input",
    "context_chips": "composer.context_chips",
    "context_chip_cursor": "composer.context_chip_cursor",
    "at_mention": "composer.at_mention",
    "pending_image_parts": "composer.pending_image_parts",
    "prompt_history": "composer.prompt_history",
    "vil_expr_lint": "composer.vil_expr_lint",
    "selection_state": "composer.selection_state",

    # Messages & Transcript (transcript)
    "messages": "transcript.messages",
    "pending_user_messages": "transcript.pending_user_messages",
    "todos": "transcript.todos",
    "streaming": "transcript.streaming",
    "rate_limit": "transcript.rate_limit",

    # Session (session)
    "session_id": "session.session_id",
    "sessions": "session.sessions",
    "session_meta": "session.session_meta",

    # Workspace & Files (workspace)
    "file_index": "workspace.file_index",
    "file_picker": "workspace.file_picker",
    "modified_files": "workspace.modified_files",
    "changeset_store": "workspace.changeset_store",
    "changeset_ui": "workspace.changeset_ui",
    "review": "workspace.review",
    "plan": "workspace.plan",

    # VIL Domain (vil_domain)
    "vil": "vil_domain.vil",
    "vil_dev": "vil_domain.vil_dev",
    "vwfd_inspector": "vil_domain.vwfd_inspector",

    # Execution & Tools (execution)
    "shell": "execution.shell",
    "runtime": "execution.runtime",
    "bridge": "execution.bridge",
    "mcp_maps": "execution.mcp_maps",
    "approvals": "execution.approvals",
    "task_tray": "execution.task_tray",
    "queue_metrics": "execution.queue_metrics",
    "activity": "execution.activity",

    # Operator & Config (operator_config)
    "operator": "operator_config.operator",
    "billing": "operator_config.billing",
}

def refactor_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    # Create a regex to match state.<field>
    # Handle state.<field>, mut_state.<field>, self.<field> if self is AppState
    # Let's just match any `\.([a-zA-Z_0-9]+)` that follows `state`, `restored`, etc.
    # Actually, a simpler way is to replace `.field` with `.domain.field` everywhere,
    # but that might break other structs that have the same field name (e.g. `foo.messages`).
    # Let's use `\b(state|restored|app_state|s|mut_state)\.([a-zA-Z_0-9]+)\b`
    
    # This might be tricky because we have many variables named state.
    # Let's just search for `.field` and if it is in the map, replace it? No, that's too aggressive.
    # What are the common names for AppState variables?
    # state, mut_state, restored, app_state, prev_state, new_state, app.
    
    # We can also do a pass using ripgrep to see variable names.
    
    # For now, let's replace `state.field`, `restored.field`, `app.field`
    pattern = re.compile(r'\b(state|restored|app_state|app|mut_state|s)\.([a-zA-Z_0-9]+)\b')
    
    def replacer(match):
        var_name = match.group(1)
        field_name = match.group(2)
        if field_name in field_map:
            return f"{var_name}.{field_map[field_name]}"
        return match.group(0)
        
    new_content = pattern.sub(replacer, content)
    
    if new_content != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Updated {filepath}")

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            refactor_file(os.path.join(root, file))
