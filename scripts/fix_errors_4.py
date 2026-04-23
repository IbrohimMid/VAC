import re
import subprocess
import os

res = subprocess.run(["cargo", "check", "-p", "vac_tui_runtime", "--tests"], capture_output=True, text=True)
err = res.stderr

# We look for "no field `X` on type" and "help: one of the expressions' fields has a field of the same name"
# Followed by the line replacement.

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            new_content = content
            new_content = re.sub(r'(\n\s*)\.mcp_maps\b', r'\1.execution.mcp_maps', new_content)
            new_content = re.sub(r'(\n\s*)\.composer\b', r'\1.composer', new_content) # wait, composer is fine
            new_content = re.sub(r'(\n\s*)\.vil\b', r'\1.vil_domain.vil', new_content)
            new_content = re.sub(r'(\n\s*)\.workbench_chrome\b', r'\1.layout.workbench_chrome', new_content)
            new_content = re.sub(r'(\n\s*)\.billing\b', r'\1.operator_config.billing', new_content)
            new_content = re.sub(r'(\n\s*)\.toasts\b', r'\1.layout.toasts', new_content)
            new_content = re.sub(r'(\n\s*)\.vil_dev\b', r'\1.vil_domain.vil_dev', new_content)
            new_content = re.sub(r'(\n\s*)\.paste\b', r'\1.layout.paste', new_content)
            new_content = re.sub(r'(\n\s*)\.streaming\b', r'\1.transcript.streaming', new_content)
            new_content = re.sub(r'(\n\s*)\.file_index\b', r'\1.workspace.file_index', new_content)
            new_content = re.sub(r'(\n\s*)\.file_picker\b', r'\1.workspace.file_picker', new_content)
            new_content = re.sub(r'(\n\s*)\.session_resume\b', r'\1.layout.session_resume', new_content)
            new_content = re.sub(r'(\n\s*)\.at_mention\b', r'\1.composer.at_mention', new_content)
            new_content = re.sub(r'(\n\s*)\.shell\b', r'\1.execution.shell', new_content)
            
            if new_content != content:
                with open(path, 'w') as f:
                    f.write(new_content)
                print(f"Fixed {path}")

fixes = [
    # textarea.rs
    ("/workspace/crates/vac_tui_runtime/src/services/textarea.rs", r'self\.composer\.input\(c\);', 'self.input(c);'),

    # workbench/approvals.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\|v: &String\| v\.clone\(\)', '|v| v.clone()'),
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\.and_then\(\|v\| v\.clone\(\)\)', '.and_then(|v: &String| Some(v.clone()))'),

    # workbench/review.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\|p: &String\| p\.ends_with', '|p: &&String| p.ends_with'),

    # services/side_panel.rs
    ("/workspace/crates/vac_tui_runtime/src/services/side_panel.rs", r'Span::raw\(ident\.clone\(\)\)', 'Span::raw(ident.to_string())'),
    ("/workspace/crates/vac_tui_runtime/src/services/side_panel.rs", r'\|s\| s\.is_connected\(\)', '|s: &&crate::mcp::McpServerState| s.is_connected()'),

    # view/overlays.rs
    ("/workspace/crates/vac_tui_runtime/src/view/overlays.rs", r'\|\(i, path\)\|', '|(i, path): (usize, &String)|'),

    # view/pickers.rs
    ("/workspace/crates/vac_tui_runtime/src/view/pickers.rs", r'\|\(i, path\)\|', '|(i, path): (usize, &std::path::PathBuf)|'),
    ("/workspace/crates/vac_tui_runtime/src/view/pickers.rs", r'\|n\| n\.to_str\(\)', '|n: &std::ffi::OsStr| n.to_str()'),

    # view/popups.rs
    ("/workspace/crates/vac_tui_runtime/src/view/popups.rs", r'\|\(i, path\)\|', '|(i, path): (usize, &String)|'),

    # view/operator.rs
    ("/workspace/crates/vac_tui_runtime/src/view/operator.rs", r'\|start\| \{', '|start: &std::time::Instant| {'),
    ("/workspace/crates/vac_tui_runtime/src/view/operator.rs", r'\|s\| s\.is_connected\(\)', '|s: &&crate::mcp::McpServerState| s.is_connected()'),
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

