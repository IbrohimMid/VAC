#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/llm_config_is_reexport.rs");
}
