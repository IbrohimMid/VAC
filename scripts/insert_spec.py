import os
import re

def insert_spec():
    src_dir = 'crates/vac_tools/src'
    for root, dirs, files in os.walk(src_dir):
        for file in files:
            if file.endswith('.rs'):
                filepath = os.path.join(root, file)
                with open(filepath, 'r') as f:
                    content = f.read()
                
                pattern = r'impl\s+(crate::registry::)?VilTool\s+for\s+\w+\s*\{'
                if re.search(pattern, content):
                    # Check if spec is already implemented
                    if 'fn spec(&self)' in content:
                        continue
                    
                    def repl(m):
                        impl_line = m.group(0)
                        spec_impl = "\n    fn spec(&self) -> vac_tool_core::ToolSpec {\n        crate::registry::default_spec(self)\n    }\n"
                        return impl_line + spec_impl
                    
                    new_content = re.sub(pattern, repl, content)
                    if new_content != content:
                        with open(filepath, 'w') as f:
                            f.write(new_content)
                        print(f"Updated {filepath}")

if __name__ == '__main__':
    insert_spec()
