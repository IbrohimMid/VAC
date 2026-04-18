def process():
    with open("crates/vac_cli/src/tui/event_loop.rs", "r") as f:
        lines = f.readlines()
    
    # 1. Create controller.rs
    controller_lines = lines[:35] + lines[252:3135]
    
    # Make handle_input_event public
    for i in range(len(controller_lines)):
        if controller_lines[i].startswith("fn handle_input_event("):
            controller_lines[i] = controller_lines[i].replace("fn handle_input_event(", "pub fn handle_input_event(")
            break
            
    with open("crates/vac_cli/src/tui/controller.rs", "w") as f:
        f.writelines(controller_lines)
        
    # 2. Modify event_loop.rs
    event_loop_lines = lines[:252] + lines[3135:]
    for i in range(len(event_loop_lines)):
        if "handle_input_event(" in event_loop_lines[i]:
            event_loop_lines[i] = event_loop_lines[i].replace("handle_input_event(", "crate::tui::controller::handle_input_event(")
            
    with open("crates/vac_cli/src/tui/event_loop.rs", "w") as f:
        f.writelines(event_loop_lines)

    # 3. Modify mod.rs
    with open("crates/vac_cli/src/tui/mod.rs", "r") as f:
        mod_lines = f.readlines()
        
    # Insert new mods before pub mod app;
    for i in range(len(mod_lines)):
        if "pub mod app;" in mod_lines[i]:
            mod_lines.insert(i, "pub mod controller;\npub mod action_registry;\npub mod overlay;\n")
            break
            
    with open("crates/vac_cli/src/tui/mod.rs", "w") as f:
        f.writelines(mod_lines)

process()
