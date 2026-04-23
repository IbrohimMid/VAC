import re

path = '/workspace/crates/vac_cli/tests/tui_flows.rs'
with open(path, 'r') as f:
    content = f.read()

new_content = re.sub(r'(\b)(state|mut_state|ctx\.state|restored)\.pending_user_messages\b', r'\1\2.transcript.pending_user_messages', content)
new_content = re.sub(r'(\n\s*)\.pending_user_messages\b', r'\1.transcript.pending_user_messages', new_content)

with open(path, 'w') as f:
    f.write(new_content)

