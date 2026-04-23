import re
import subprocess

res = subprocess.run(["cargo", "check", "-p", "vac_tui_runtime", "--tests"], capture_output=True, text=True)
err = res.stderr

# We parse "no field `field_name` on type `type_name`"
# And "help: one of the expressions' fields has a field of the same name ... .domain.field_name"
# Then we apply that domain.field_name to the file.

import os

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            new_content = content
            new_content = re.sub(r'(\n\s*)\.overlay_manager\b', r'\1.layout.overlay_manager', new_content)
            new_content = re.sub(r'(\n\s*)\.command_palette\b', r'\1.layout.command_palette', new_content)
            new_content = re.sub(r'(\n\s*)\.switchers\b', r'\1.layout.switchers', new_content)
            new_content = re.sub(r'(\n\s*)\.changeset_store\b', r'\1.workspace.changeset_store', new_content)
            new_content = re.sub(r'(\n\s*)\.review\b', r'\1.workspace.review', new_content)
            new_content = re.sub(r'(\n\s*)\.operator\b', r'\1.operator_config.operator', new_content)
            new_content = re.sub(r'(\n\s*)\.side_panel\b', r'\1.layout.side_panel', new_content)
            new_content = re.sub(r'(\n\s*)\.todos\b', r'\1.transcript.todos', new_content)
            new_content = re.sub(r'(\n\s*)\.theme\b', r'\1.core.theme', new_content)
            new_content = re.sub(r'(\n\s*)\.runtime\b', r'\1.execution.runtime', new_content)
            new_content = re.sub(r'(\n\s*)\.approvals\b', r'\1.execution.approvals', new_content)
            new_content = re.sub(r'(\n\s*)\.sessions\b', r'\1.session.sessions', new_content)
            
            # And also fix `app.field` or `s.field`
            # For the ones that were missed because they had `state\n.field` we just fixed above.
            
            if new_content != content:
                with open(path, 'w') as f:
                    f.write(new_content)
                print(f"Fixed {path}")
