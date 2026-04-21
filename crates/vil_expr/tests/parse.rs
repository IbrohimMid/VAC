use vil_expr::{ast::*, parse};

#[test]
fn parse_nested_field_access() {
    let expr = parse("a.b.c").unwrap();
    assert_eq!(
        expr,
        Expr::FieldAccess(
            Box::new(Expr::FieldAccess(
                Box::new(Expr::Ident("a".to_string())),
                "b".to_string()
            )),
            "c".to_string()
        )
    );
}

#[test]
fn parse_call_with_args() {
    let expr = parse("foo(1, true, \"bar\")").unwrap();
    assert_eq!(
        expr,
        Expr::Call(
            Box::new(Expr::Ident("foo".to_string())),
            vec![
                Expr::Literal(Literal::Int(1)),
                Expr::Literal(Literal::Bool(true)),
                Expr::Literal(Literal::Str("bar".to_string())),
            ]
        )
    );
}

#[test]
fn parse_operator_precedence() {
    let expr = parse("1 + 2 * 3 == 7").unwrap();
    assert_eq!(
        expr,
        Expr::BinOp(
            Box::new(Expr::BinOp(
                Box::new(Expr::Literal(Literal::Int(1))),
                BinOp::Add,
                Box::new(Expr::BinOp(
                    Box::new(Expr::Literal(Literal::Int(2))),
                    BinOp::Mul,
                    Box::new(Expr::Literal(Literal::Int(3)))
                ))
            )),
            BinOp::Eq,
            Box::new(Expr::Literal(Literal::Int(7)))
        )
    );
}

#[test]
fn parse_ternary() {
    let expr = parse("a ? b : c").unwrap();
    assert_eq!(
        expr,
        Expr::Ternary(
            Box::new(Expr::Ident("a".to_string())),
            Box::new(Expr::Ident("b".to_string())),
            Box::new(Expr::Ident("c".to_string()))
        )
    );
}

#[test]
fn parse_index() {
    let expr = parse("a[0]").unwrap();
    assert_eq!(
        expr,
        Expr::Index(
            Box::new(Expr::Ident("a".to_string())),
            Box::new(Expr::Literal(Literal::Int(0)))
        )
    );
}

#[test]
fn corpus_parsing() {
    let exprs = [
        "1 == 1",
        "user.age >= 18",
        "!is_admin",
        "has_role(\"admin\") && is_active",
        "data.items[0].id == null",
        "a + b - c * d / e",
        "a > b || c <= d",
        "a ? b : c ? d : e",
        "foo()",
        "foo.bar()",
        "foo().bar",
        "1.5 + 2.5",
        "true != false",
        "\"foo\" == 'foo'",
        "user.name == \"John\"",
        "user.roles[0] == \"admin\"",
        "-1 + 2",
        "a * -b",
        "!(a == b)",
        "a == b ? true : false",
    ];
    for e in exprs {
        assert!(parse(e).is_ok(), "Failed to parse: {}", e);
    }
}
