with open('.vac/agent-journal.md', 'r') as f:
    lines = f.readlines()

new_lines = []
skip = False
for line in lines:
    if line.startswith('## M3 Tool spec() explicit — DONE'):
        if skip: continue
        skip = True
        
with open('.vac/agent-journal.md', 'w') as f:
    f.writelines(new_lines)
