//! Native handler scaffold templates.

pub fn render_handler_module(name: &str) -> String {
    format!(
        "use vil_server::prelude::*;\n\n#[vil_handler]\npub async fn run(ctx: ServiceCtx) -> VilResponse<String> {{\n    let _ = ctx;\n    VilResponse::ok(\"{name} is ready\".to_string())\n}}\n"
    )
}
