import re
import os

fixes = [
    # app/types/shell.rs
    ("/workspace/crates/vac_tui_runtime/src/app/types/shell.rs", r'self\.session\.sessions', 'self.sessions'),

    # workbench/approvals.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/approvals.rs", r'\|v: &String\| v\.clone\(\)', '|v| v.clone()'),

    # workbench/review.rs
    ("/workspace/crates/vac_tui_runtime/src/workbench/review.rs", r'\|p: &String\| p\.ends_with', '|p| p.ends_with'),

    # app/types/helpers.rs
    ("/workspace/crates/vac_tui_runtime/src/app/types/helpers.rs", r'\.layout\.command_palette\.recent_commands', '.command_palette.recent_commands'),
    ("/workspace/crates/vac_tui_runtime/src/app/types/helpers.rs", r'\.layout\.switchers\.available_models', '.switchers.available_models'),

    # view/mod.rs
    ("/workspace/crates/vac_tui_runtime/src/view/mod.rs", r'\.layout\.overlay_manager', '.overlay_manager'),
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
