import re

with open('/workspace/crates/vac_cli/src/commands/run.rs', 'r') as f:
    content = f.read()

# Remove EngineMode struct and impl
content = re.sub(r'/// Engine selector for `vac run`\.[\s\S]*?impl EngineMode \{[\s\S]*?\}\n\n', '', content)

# Remove engine_mode param from execute
content = content.replace('targets: Vec<String>,\n    engine_mode: EngineMode,\n) -> anyhow::Result<()> {', 'targets: Vec<String>,\n) -> anyhow::Result<()> {')

# Remove engine_mode_tests
content = re.sub(r'#\[cfg\(test\)\]\nmod engine_mode_tests \{[\s\S]*?\}\n', '', content)

with open('/workspace/crates/vac_cli/src/commands/run.rs', 'w') as f:
    f.write(content)

