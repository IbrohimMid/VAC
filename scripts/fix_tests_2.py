import re
import os

for root, dirs, files in os.walk('/workspace/crates/vac_tui_runtime/tests'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            new_content = content
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.quit\b', r'\1\2.core.quit', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.toasts\b', r'\1\2.layout.toasts', new_content)

            if new_content != content:
                with open(path, 'w') as f:
                    f.write(new_content)
                print(f"Fixed {path}")

