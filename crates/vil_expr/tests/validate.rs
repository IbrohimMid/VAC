use vil_expr::{parse, validate, SymbolTable};

#[test]
fn validate_rejects_v_cel_legacy_terms() {
    let input = "v-cel";
    let expr = parse(input).unwrap();
    let symbols = SymbolTable::new();
    let report = validate(&expr, input, &symbols);
    assert!(!report.is_valid());
    assert!(report
        .issues
        .iter()
        .any(|i| i.message.contains("legacy alias 'v-cel'")));
}

#[test]
fn validate_unknown_identifier_errors() {
    let input = "unknown_ident";
    let expr = parse(input).unwrap();
    let symbols = SymbolTable::new();
    let report = validate(&expr, input, &symbols);
    assert!(!report.is_valid());
    assert!(report
        .issues
        .iter()
        .any(|i| i.message.contains("Unknown identifier: unknown_ident")));
}
#[test]
fn validate_accepts_valid_expr() {
    let input = "a.b == c[0] && foo(1)";
    let expr = parse(input).unwrap();
    let symbols = SymbolTable::new();
    assert!(validate(&expr, input, &symbols).is_valid());
}
