//! WASM handler scaffold templates.

pub fn render_cargo_toml(name: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nwasm-bindgen = \"0.2\"\n"
    )
}

pub fn render_lib_rs(name: &str) -> String {
    format!(
        "use wasm_bindgen::prelude::*;\n\n#[wasm_bindgen]\npub fn run(input: String) -> String {{\n    input\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn wasm_scaffold_compiles_in_principle() {{\n        assert_eq!(run(\"{name}\".to_string()), \"{name}\");\n    }}\n}}\n"
    )
}
