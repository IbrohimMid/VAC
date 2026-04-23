import re
import subprocess
import os

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            new_content = content
            new_content = re.sub(r'(\n\s*)\.message_ui\b', r'\1.layout.message_ui', new_content)
            new_content = re.sub(r'(\n\s*)\.ask_user\b', r'\1.layout.ask_user', new_content)
            new_content = re.sub(r'(\n\s*)\.modified_files\b', r'\1.workspace.modified_files', new_content)
            new_content = re.sub(r'(\n\s*)\.session_meta\b', r'\1.session.session_meta', new_content)
            new_content = re.sub(r'(\n\s*)\.commands\b', r'\1.layout.commands', new_content)
            new_content = re.sub(r'(\n\s*)\.pending_user_messages\b', r'\1.transcript.pending_user_messages', new_content)
            new_content = re.sub(r'(\n\s*)\.vil_expr_lint\b', r'\1.composer.vil_expr_lint', new_content)
            new_content = re.sub(r'(\n\s*)\.input\b', r'\1.composer.input', new_content)
            
            if new_content != content:
                with open(path, 'w') as f:
                    f.write(new_content)
                print(f"Fixed fields {path}")

fixes = [
    # input_commands/tests.rs
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_commands/tests.rs", r'let mut unregistered = Vec::new\(\);', 'let mut unregistered: Vec<&str> = Vec::new();'),
    ("/workspace/crates/vac_tui_runtime/src/handlers/input_commands/tests.rs", r'unregistered\.push\(\*cmd\);', 'unregistered.push(cmd);'),

    # view/operator.rs
    ("/workspace/crates/vac_tui_runtime/src/view/operator.rs", r'\|start: &std::time::Instant\|', '|start|'),

    # workbench/approvals.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\.and_then\(\|v: &String\| Some\(v\.clone\(\)\)\)', '.and_then(|v| v.clone())'),
    
    # review.rs - fixing type annotations
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\|p: &&String\|', '|p|'),

    # popups.rs / pickers.rs / overlays.rs
    ("/workspace/crates/vac_tui_runtime/src/view/popups.rs", r'\|\(i, path\): \(usize, &String\)\|', '|(i, path)|'),
    ("/workspace/crates/vac_tui_runtime/src/view/pickers.rs", r'\|\(i, path\): \(usize, &std::path::PathBuf\)\|', '|(i, path)|'),
    ("/workspace/crates/vac_tui_runtime/src/view/overlays.rs", r'\|\(i, path\): \(usize, &String\)\|', '|(i, path)|'),
]

for path, pattern, replacement in fixes:
    try:
        with open(path, 'r', encoding='utf-8') as f:
            content = f.read()
        new_content = re.sub(pattern, replacement, content)
        if new_content != content:
            with open(path, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"Fixed regex {path}: {pattern}")
    except Exception as e:
        print(f"Error on {path}: {e}")

