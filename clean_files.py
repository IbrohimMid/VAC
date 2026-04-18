import re

def process_controller():
    with open("crates/vac_cli/src/tui/controller.rs", "r") as f:
        content = f.read()
    
    # Remove run_tui
    start = content.find("pub async fn run_tui")
    if start != -1:
        # Find the end of run_tui. It's a huge function, let's find fn handle_paste_tray_key
        end = content.find("fn handle_paste_tray_key")
        if end != -1:
            # But wait, there might be comments. Let's find the last '}' before handle_paste_tray_key
            content = content[:start] + content[end:]
    
    # rename fn handle_input_event to pub fn handle_input_event
    content = content.replace("fn handle_input_event(", "pub fn handle_input_event(")
    
    with open("crates/vac_cli/src/tui/controller.rs", "w") as f:
        f.write(content)

def process_event_loop():
    with open("crates/vac_cli/src/tui/event_loop.rs", "r") as f:
        content = f.read()
    
    # We want to keep run_tui, and remove handle_paste_tray_key, handle_input_event, etc.
    # So we remove everything from "fn handle_paste_tray_key" down to "fn make_state" (exclusive)
    start = content.find("fn handle_paste_tray_key")
    if start != -1:
        end = content.find("#[cfg(test)]")
        if end != -1:
            content = content[:start] + content[end:]
            
    # Replace handle_input_event with crate::tui::controller::handle_input_event
    content = content.replace("handle_input_event(", "crate::tui::controller::handle_input_event(")
    
    with open("crates/vac_cli/src/tui/event_loop.rs", "w") as f:
        f.write(content)

process_controller()
process_event_loop()
