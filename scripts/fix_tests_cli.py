import re
import os

for root, dirs, files in os.walk('/workspace/crates/vac_cli/tests'):
    for file in files:
        if file.endswith('.rs'):
            path = os.path.join(root, file)
            with open(path, 'r') as f:
                content = f.read()
            
            new_content = content
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.vil_dev\b', r'\1\2.vil_domain.vil_dev', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.vil\b', r'\1\2.vil_domain.vil', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.runtime\b', r'\1\2.execution.runtime', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.shell\b', r'\1\2.execution.shell', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.activity\b', r'\1\2.execution.activity', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.changeset_ui\b', r'\1\2.workspace.changeset_ui', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.review\b', r'\1\2.workspace.review', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.messages\b', r'\1\2.transcript.messages', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.todos\b', r'\1\2.transcript.todos', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.file_index\b', r'\1\2.workspace.file_index', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.file_picker\b', r'\1\2.workspace.file_picker', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.operator\b', r'\1\2.operator_config.operator', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.approvals\b', r'\1\2.execution.approvals', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.theme\b', r'\1\2.core.theme', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.changeset_store\b', r'\1\2.workspace.changeset_store', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.modified_files\b', r'\1\2.workspace.modified_files', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.session_meta\b', r'\1\2.session.session_meta', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.session_id\b', r'\1\2.session.session_id', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.sessions\b', r'\1\2.session.sessions', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.mcp_maps\b', r'\1\2.execution.mcp_maps', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.streaming\b', r'\1\2.transcript.streaming', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.input\b', r'\1\2.composer.input', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.lsp_ui\b', r'\1\2.layout.lsp_ui', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.focus\b', r'\1\2.layout.focus', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.workbench_tab\b', r'\1\2.layout.workbench_tab', new_content)
            new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.workbench_chrome\b', r'\1\2.layout.workbench_chrome', new_content)
            
            # also handle \n.field
            new_content = re.sub(r'(\n\s*)\.vil_dev\b', r'\1.vil_domain.vil_dev', new_content)
            new_content = re.sub(r'(\n\s*)\.activity\b', r'\1.execution.activity', new_content)
            new_content = re.sub(r'(\n\s*)\.runtime\b', r'\1.execution.runtime', new_content)
            new_content = re.sub(r'(\n\s*)\.messages\b', r'\1.transcript.messages', new_content)
            new_content = re.sub(r'(\n\s*)\.workbench_chrome\b', r'\1.layout.workbench_chrome', new_content)

            if new_content != content:
                with open(path, 'w') as f:
                    f.write(new_content)
                print(f"Fixed {path}")

