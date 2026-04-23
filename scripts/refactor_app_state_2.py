import os
import re
from refactor_app_state import field_map

def refactor_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    pattern = re.compile(r'\b(self)\.([a-zA-Z_0-9]+)\b')
    
    def replacer(match):
        var_name = match.group(1)
        field_name = match.group(2)
        if field_name in field_map:
            return f"{var_name}.{field_map[field_name]}"
        return match.group(0)
        
    new_content = pattern.sub(replacer, content)
    
    # Also fix some chained things that might have been missed
    # Like `state.operator.current_model` where `operator` is the field
    # Wait, `state.operator` was replaced with `state.operator_config.operator`.
    # Let's just fix `self.`
    if new_content != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Updated {filepath}")

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/src'):
    for file in files:
        if file.endswith('.rs'):
            refactor_file(os.path.join(root, file))
