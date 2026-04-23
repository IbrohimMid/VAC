import re

fixes = [
    # services/side_panel.rs
    ("/workspace/crates/vac_tui_runtime/src/services/side_panel.rs", r'\|s: &&crate::mcp::McpServerState\|', '|s|'),
    
    # view/operator.rs
    ("/workspace/crates/vac_tui_runtime/src/view/operator.rs", r'\|s: &&crate::mcp::McpServerState\|', '|s|'),

    # runner/shell_dispatch.rs
    ("/workspace/crates/vac_tui_runtime/src/runner/shell_dispatch.rs", r'\.execution\.runtime', '.runtime'),

    # runner.rs
    ("/workspace/crates/vac_tui_runtime/src/runner.rs", r'\.workspace\.file_index', '.file_index'),
    ("/workspace/crates/vac_tui_runtime/src/runner.rs", r'\|path: /\* Type \*/\|', '|path|'),
    ("/workspace/crates/vac_tui_runtime/src/runner.rs", r'\|path\| path\.to_string_lossy\(\)\.to_string\(\)', '|path: &std::path::PathBuf| path.to_string_lossy().to_string()'),

    # handlers/input_commands.rs
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_commands.rs", r'(\n\s*)\.messages\b', r'\1.transcript.messages'),

    # handlers/input_core.rs
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_core.rs", r'(\n\s*)\.quit\b', r'\1.core.quit'),
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_core.rs", r'(\n\s*)\.messages\b', r'\1.transcript.messages'),
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_core.rs", r'(\n\s*)\.banner\b', r'\1.layout.banner'),
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_core.rs", r'\|m: /\* Type \*/\|', '|m|'),
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_core.rs", r'\|m\| !m\.is_expired\(\)', '|m: &crate::services::banner::BannerMessage| !m.is_expired()'),
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
        pass

