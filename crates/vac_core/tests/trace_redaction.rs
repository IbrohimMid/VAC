use vac_core::security::SecretSubstitution;

#[test]
fn trace_containing_secrets_exports_zero_raw_secrets() {
    let secrets_and_values: &[(&str, &str)] = &[
        // (full line, value that must be redacted)
        ("AWS key: AKIAIOSFODNN7EXAMPLE", "AKIAIOSFODNN7EXAMPLE"),
        ("token: sk_test_1234567890abcdefghij", "sk_test_1234567890abcdefghij"),
        ("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
         "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
        ("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef12", "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef12"),
        ("glpat-xxxxxxxxxxxxxxxxxxxx", "glpat-xxxxxxxxxxxxxxxxxxxx"),
        ("sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz", "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz"),
        ("Server IP: 192.168.1.100", "192.168.1.100"),
    ];

    let mut payload = String::new();
    for (line, _) in secrets_and_values {
        payload.push_str(&format!("Tool call output: {}\n", line));
    }

    let mut sub = SecretSubstitution::new();
    let redacted = sub.substitute(&payload);

    for (_, secret_value) in secrets_and_values {
        assert!(
            !redacted.contains(secret_value),
            "Redacted output must not contain raw secret: {:?}",
            secret_value
        );
    }
}

#[test]
fn redaction_applied_to_crash_json_payload() {
    let raw_panic = "panicked at 'API call failed with key=AKIAIOSFODNN7EXAMPLE'";
    let mut sub = SecretSubstitution::new();
    let redacted = sub.substitute(raw_panic);
    assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn redaction_preserves_non_secret_content() {
    let text = "File src/main.rs modified successfully. 42 lines changed.";
    let mut sub = SecretSubstitution::new();
    let result = sub.substitute(text);
    assert_eq!(result, text);
}
