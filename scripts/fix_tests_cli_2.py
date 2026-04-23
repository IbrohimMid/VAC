import re
import os

path = '/workspace/crates/vac_cli/tests/tui_flows.rs'
with open(path, 'r') as f:
    content = f.read()

new_content = content
new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.overlay_manager\b', r'\1\2.layout.overlay_manager', new_content)
new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.command_palette\b', r'\1\2.layout.command_palette', new_content)
new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.queue_metrics\b', r'\1\2.execution.queue_metrics', new_content)
new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.switchers\b', r'\1\2.layout.switchers', new_content)

new_content = re.sub(r'(\n\s*)\.overlay_manager\b', r'\1.layout.overlay_manager', new_content)
new_content = re.sub(r'(\n\s*)\.workbench_chrome\b', r'\1.layout.workbench_chrome', new_content)

if new_content != content:
    with open(path, 'w') as f:
        f.write(new_content)
    print(f"Fixed {path}")

