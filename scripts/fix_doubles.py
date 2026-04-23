import re
import os

def fix_doubles(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    new_content = content.replace('.core.core.', '.core.')
    new_content = new_content.replace('.layout.layout.', '.layout.')
    new_content = new_content.replace('.composer.composer.', '.composer.')
    new_content = new_content.replace('.transcript.transcript.', '.transcript.')
    new_content = new_content.replace('.session.session.', '.session.')
    new_content = new_content.replace('.workspace.workspace.', '.workspace.')
    new_content = new_content.replace('.vil_domain.vil_domain.', '.vil_domain.')
    new_content = new_content.replace('.execution.execution.', '.execution.')
    new_content = new_content.replace('.operator_config.operator_config.', '.operator_config.')
    
    # Also fix some specific inference errors
    # |v| v.clone() -> |v: &String| v.clone()
    new_content = re.sub(r'\|v\| v\.clone\(\)', '|v: &String| v.clone()', new_content)
    # for line in summary.lines()
    # lines.push(Line::raw(line.to_string()));
    # If summary is known, maybe it doesn't know what summary is.
    
    # Span::raw(cp.clone()) -> Span::raw(cp.to_string())
    new_content = re.sub(r'Span::raw\(cp\.clone\(\)\)', 'Span::raw(cp.to_string())', new_content)

    if new_content != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Fixed {filepath}")

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            fix_doubles(os.path.join(root, file))
