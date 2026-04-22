import re

with open("crates/vac_cli/src/tui/controller.rs", "r") as f:
    lines = f.readlines()

imports = []
controller_lines = []
input_lines = []
update_lines = []

current_dest = imports

for line in lines:
    if line.startswith("/// Rulebook configuration"):
        current_dest = controller_lines
    elif line.startswith("fn handle_paste_tray_key"):
        current_dest = input_lines
    elif line.startswith("pub fn handle_input_event"):
        current_dest = input_lines
    elif line.startswith("fn message_at_row"):
        current_dest = input_lines
    elif line.startswith("fn estimate_context_percent"):
        current_dest = update_lines
    elif line.startswith("fn plan_open_editor"):
        current_dest = input_lines
    elif line.startswith("fn plan_write_status"):
        current_dest = input_lines
    elif line.startswith("pub(crate) fn open_ask_user_popup"):
        current_dest = update_lines
    elif line.startswith("fn is_low_risk_tool"):
        current_dest = update_lines
    elif line.startswith("fn is_vil_tool"):
        current_dest = update_lines
    elif line.startswith("fn truncate_banner_text"):
        current_dest = update_lines
    elif line.startswith("pub(crate) fn classify_critical_banner"):
        current_dest = update_lines
    elif line.startswith("fn push_banner_direct"):
        current_dest = update_lines
    elif line.startswith("fn policy_gate_allows_shell_command"):
        current_dest = update_lines
    elif line.startswith("pub fn handle_backend_event"):
        current_dest = update_lines
    elif line.startswith("pub fn dispatch_builtin_command"):
        current_dest = input_lines
        
    current_dest.append(line)

# Let's fix pub(crate) on functions that need to be shared
def make_pub_crate(lines, fn_name):
    for i, line in enumerate(lines):
        if line.startswith(f"fn {fn_name}"):
            lines[i] = f"pub(crate) " + line
            break

make_pub_crate(update_lines, "policy_gate_allows_shell_command")
make_pub_crate(update_lines, "push_banner_direct")
make_pub_crate(update_lines, "truncate_banner_text")
make_pub_crate(update_lines, "open_ask_user_popup")

# We also need imports for handlers/input.rs and update.rs
shared_imports = """
use crate::tui::app::{AppState, InputEvent, OutputEvent};
use tokio::sync::mpsc::Sender;
use crate::tui::handlers::HandlerContext;
use crate::tui::handlers::{
    approval, changeset as changeset_handler, file_search, isolation_switcher, message_action,
    model_switcher, profile_switcher, review as review_handler, rulebook_switcher,
    shell as shell_handler,
};
"""

with open("crates/vac_cli/src/tui/handlers/input.rs", "w") as f:
    f.write(shared_imports + "\n")
    f.write("use crate::tui::update::*;\n")
    f.writelines(input_lines)

with open("crates/vac_cli/src/tui/update.rs", "w") as f:
    f.write(shared_imports + "\n")
    f.write("use crate::tui::handlers::input::*;\n")
    f.writelines(update_lines)

with open("crates/vac_cli/src/tui/controller.rs", "w") as f:
    f.write("pub use crate::tui::handlers::input::handle_input_event;\n")
    f.write("pub use crate::tui::update::handle_backend_event;\n\n")
    f.writelines(imports)
    f.writelines(controller_lines)

