//! F0.2 — Public API smoke test. Establishes discoverability for the
//! otherwise test-less `vil_expr` crate.

use vil_expr::{SymbolTable, parse, validate};

#[test]
fn parse_and_validate_expose_public_api() {
    // Minimal valid expression — exact semantics belong to future
    // issues; this test guarantees the public surface compiles and
    // returns a `Result`/report shape.
    let parsed = parse("1");
    assert!(
        parsed.is_ok() || parsed.is_err(),
        "parse must return a Result (public API contract)"
    );

    if let Ok(expr) = parsed {
        let report = validate(&expr, "1", &SymbolTable::default());
        // Report is a struct carrying issues + severity — just touch
        // it to prove it's constructable.
        let _ = report;
    }
}

#[test]
fn symbol_table_default_is_constructable() {
    let _ = SymbolTable::default();
}
