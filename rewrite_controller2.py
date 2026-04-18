import re

with open("crates/vac_cli/src/tui/controller.rs", "r") as f:
    content = f.read()

# For InputSubmitted
s = "if let Some(cmd) = state\n                        .commands\n                        .iter()\n                        .find(|c| c.command == cmd_word)\n                        .cloned()\n                    {"
idx = content.find(s)
if idx != -1:
    # find the matching }
    depth = 1
    i = idx + len(s)
    while i < len(content):
        if content[i] == '{':
            depth += 1
        elif content[i] == '}':
            depth -= 1
            if depth == 0:
                break
        i += 1
    
    # Check if there is an `else` block
    next_char_idx = i + 1
    while next_char_idx < len(content) and content[next_char_idx].isspace():
        next_char_idx += 1
    
    if content[next_char_idx:next_char_idx+4] == "else":
        # find the end of the else block
        j = next_char_idx + 4
        while j < len(content) and content[j].isspace():
            j += 1
        if content[j] == '{':
            depth = 1
            j += 1
            while j < len(content):
                if content[j] == '{':
                    depth += 1
                elif content[j] == '}':
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
        
        replacement = """if !dispatch_builtin_command(state, output_tx, cmd_word, cmd_args) {
                        let expanded = state.expand_pending_pastes(&msg);
                        state.add_user_message(expanded.clone());
                        let parts = std::mem::take(&mut state.pending_image_parts);
                        let _ = output_tx
                            .try_send(OutputEvent::UserMessage(expanded, None, parts, None));
                    }"""
        
        content = content[:idx] + replacement + content[j+1:]
        with open("crates/vac_cli/src/tui/controller.rs", "w") as f:
            f.write(content)

