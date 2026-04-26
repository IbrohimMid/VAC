//! D9 — projection contract tests.
//!
//! Each test pins one operator-visible invariant:
//!
//! 1–4.  severity mapping (ok/warning/error/cancelled)
//! 5.    pending status when result row missing
//! 6.    payload + arguments are NEVER rendered into projection text
//! 7.    multi-tool order preserved
//! 8.    summary counts add up correctly
//! 9.    missing transcript file → empty projection + zeroed summary
//! 10.   pre-D7E transcript (no tool rows) → empty projection
//! 11.   activity entry detail is operator-friendly + redaction-safe

use vac_shell_contracts::{Severity, ShellActivityKind};
use vac_shell_host_transcript_projection::{
    ToolUseStatus, project_tool_use_activity, session_tool_use_summary, summarize_tool_use,
};
use vac_shell_test_support::{tool_call_json_line as tool_call_line, tool_result_json_line as tool_result_line, write_jsonl_body as write_jsonl};

// ---------------------------------------------------------------------
// 1–4. Severity / status mapping.
// ---------------------------------------------------------------------

#[test]
fn ok_envelope_projects_to_ok_status_and_severity_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ok.jsonl");
    let body = format!(
        "{}\n{}\n",
        tool_call_line("a", "alpha", serde_json::json!({"i": 1})),
        tool_result_line(
            "a",
            "alpha",
            "ok",
            "alpha ok",
            serde_json::json!({"hit": true}),
            12,
        ),
    );
    write_jsonl(&path, &body);

    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj.len(), 1);
    assert_eq!(proj[0].status, ToolUseStatus::Ok);
    assert_eq!(proj[0].duration_ms, 12);
    assert_eq!(proj[0].summary, "alpha ok");
    let entry = proj[0].to_activity_entry(0);
    assert_eq!(entry.severity, Severity::Ok);
    assert_eq!(entry.kind, ShellActivityKind::ToolResult);
}

#[test]
fn warning_envelope_projects_to_warning_status_and_severity_warn() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("w.jsonl");
    let body = format!(
        "{}\n{}\n",
        tool_call_line("w1", "warner", serde_json::json!({})),
        tool_result_line(
            "w1",
            "warner",
            "warning",
            "soft warning",
            serde_json::json!({"note": "soft"}),
            5,
        ),
    );
    write_jsonl(&path, &body);
    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj[0].status, ToolUseStatus::Warning);
    assert_eq!(proj[0].to_activity_entry(0).severity, Severity::Warn);
}

#[test]
fn error_envelope_projects_to_error_status_and_severity_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("e.jsonl");
    let body = format!(
        "{}\n{}\n",
        tool_call_line("e1", "boomer", serde_json::json!({})),
        tool_result_line(
            "e1",
            "boomer",
            "error",
            "boomer failed",
            serde_json::json!({"error": "kaboom"}),
            7,
        ),
    );
    write_jsonl(&path, &body);
    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj[0].status, ToolUseStatus::Error);
    assert_eq!(proj[0].to_activity_entry(0).severity, Severity::Error);
}

#[test]
fn cancelled_envelope_projects_to_cancelled_status_with_warn_severity() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("c.jsonl");
    let body = format!(
        "{}\n{}\n",
        tool_call_line("c1", "stopper", serde_json::json!({})),
        tool_result_line(
            "c1",
            "stopper",
            "cancelled",
            "operator cancelled",
            serde_json::json!({}),
            0,
        ),
    );
    write_jsonl(&path, &body);
    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj[0].status, ToolUseStatus::Cancelled);
    assert_eq!(
        proj[0].to_activity_entry(0).severity,
        Severity::Warn,
        "cancelled must NOT be treated as Error"
    );
}

// ---------------------------------------------------------------------
// 5. Missing result → pending status.
// ---------------------------------------------------------------------

#[test]
fn missing_result_projects_to_pending_status() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("p.jsonl");
    let body = format!(
        "{}\n",
        tool_call_line("p1", "halfdone", serde_json::json!({}))
    );
    write_jsonl(&path, &body);
    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj.len(), 1);
    assert_eq!(proj[0].status, ToolUseStatus::Pending);
    assert_eq!(proj[0].duration_ms, 0);
    assert_eq!(proj[0].to_activity_entry(0).severity, Severity::Warn);
}

// ---------------------------------------------------------------------
// 6. Payload + arguments must NEVER appear in the projected text.
// ---------------------------------------------------------------------

#[test]
fn projection_redacts_payload_and_arguments() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("redact.jsonl");
    // The argument and the payload both contain a sentinel
    // string that MUST NOT appear in any projected field.
    let secret_args = "ARGS_SECRET_NEEDLE_a9f3";
    let secret_payload = "PAYLOAD_SECRET_NEEDLE_b2c4";
    let body = format!(
        "{}\n{}\n",
        tool_call_line(
            "x",
            "leaker",
            serde_json::json!({"arg": secret_args})
        ),
        tool_result_line(
            "x",
            "leaker",
            "ok",
            "leaker ok",
            serde_json::json!({"echo": secret_payload}),
            0,
        ),
    );
    write_jsonl(&path, &body);

    let proj = project_tool_use_activity(&path).unwrap();
    let entry = proj[0].to_activity_entry(0);
    let title = &entry.title;
    let detail = entry.detail.as_deref().unwrap_or("");
    let bag = format!("{title}|{detail}|{}|{}", proj[0].summary, proj[0].tool_name);
    assert!(
        !bag.contains(secret_args),
        "argument string leaked into projection: {bag}"
    );
    assert!(
        !bag.contains(secret_payload),
        "payload string leaked into projection: {bag}"
    );
    // Sanity: the redacted-on-disk file still contains them.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(on_disk.contains(secret_args));
    assert!(on_disk.contains(secret_payload));
}

// ---------------------------------------------------------------------
// 7. Multi-tool order preserved.
// ---------------------------------------------------------------------

#[test]
fn multi_tool_projection_preserves_transcript_order() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("multi.jsonl");
    let mut body = String::new();
    for id in ["a", "b", "c"] {
        body.push_str(&tool_call_line(id, id, serde_json::json!({})));
        body.push('\n');
    }
    // Result rows out-of-order on purpose; pairing should
    // still preserve call order (a, b, c).
    body.push_str(&tool_result_line("c", "c", "ok", "c ok", serde_json::json!({}), 0));
    body.push('\n');
    body.push_str(&tool_result_line("a", "a", "ok", "a ok", serde_json::json!({}), 0));
    body.push('\n');
    body.push_str(&tool_result_line("b", "b", "error", "b failed", serde_json::json!({}), 0));
    body.push('\n');
    write_jsonl(&path, &body);

    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(
        proj.iter().map(|p| p.call_id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(proj[0].status, ToolUseStatus::Ok);
    assert_eq!(proj[1].status, ToolUseStatus::Error);
    assert_eq!(proj[2].status, ToolUseStatus::Ok);
}

// ---------------------------------------------------------------------
// 8. Summary counts.
// ---------------------------------------------------------------------

#[test]
fn summary_counts_each_status_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("counts.jsonl");
    let mut body = String::new();
    let cases: &[(&str, &str)] = &[
        ("a", "ok"),
        ("b", "ok"),
        ("c", "warning"),
        ("d", "error"),
        ("e", "error"),
        ("f", "error"),
        ("g", "cancelled"),
    ];
    for (id, _) in cases {
        body.push_str(&tool_call_line(id, "t", serde_json::json!({})));
        body.push('\n');
    }
    for (id, kind) in cases {
        body.push_str(&tool_result_line(id, "t", kind, "x", serde_json::json!({}), 0));
        body.push('\n');
    }
    // One pending: call without matching result.
    body.push_str(&tool_call_line("h", "t", serde_json::json!({})));
    body.push('\n');
    write_jsonl(&path, &body);

    let s = summarize_tool_use(&path).unwrap();
    assert_eq!(s.total_calls, 8);
    assert_eq!(s.ok_count, 2);
    assert_eq!(s.warning_count, 1);
    assert_eq!(s.error_count, 3);
    assert_eq!(s.cancelled_count, 1);
    assert_eq!(s.pending_count, 1);
    // session_tool_use_summary is the convenience alias.
    assert_eq!(session_tool_use_summary(&path).unwrap(), s);
}

// ---------------------------------------------------------------------
// 9. Missing transcript file is not an error.
// ---------------------------------------------------------------------

#[test]
fn missing_transcript_returns_empty_projection_and_zeroed_summary() {
    let path = std::env::temp_dir().join("does-not-exist-d9.jsonl");
    assert!(project_tool_use_activity(&path).unwrap().is_empty());
    let s = summarize_tool_use(&path).unwrap();
    assert_eq!(s.total_calls, 0);
    assert_eq!(s.ok_count, 0);
    assert_eq!(s.error_count, 0);
}

// ---------------------------------------------------------------------
// 10. Pre-D7E transcript without tool rows still parses cleanly.
// ---------------------------------------------------------------------

#[test]
fn old_transcript_without_tool_rows_returns_empty_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("old.jsonl");
    let body = r#"{"id":"00000000-0000-0000-0000-000000000001","session_id":"00000000-0000-0000-0000-000000000002","kind":"accepted","timestamp":"2026-04-26T00:00:00Z","content":{"input":"hi","submitted_at":"2026-04-26T00:00:00Z","metadata":null}}
{"id":"00000000-0000-0000-0000-000000000003","session_id":"00000000-0000-0000-0000-000000000002","kind":"finished","timestamp":"2026-04-26T00:00:00Z","content":{"via":"llm","usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}
"#;
    write_jsonl(&path, body);
    assert!(project_tool_use_activity(&path).unwrap().is_empty());
    let s = summarize_tool_use(&path).unwrap();
    assert_eq!(s.total_calls, 0);
}

// ---------------------------------------------------------------------
// 11. Activity entry detail is operator-friendly and bounded.
// ---------------------------------------------------------------------

#[test]
fn activity_entry_detail_only_shows_summary_duration_and_transcript_path() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("detail.jsonl");
    let body = format!(
        "{}\n{}\n",
        tool_call_line("only", "alpha", serde_json::json!({})),
        tool_result_line("only", "alpha", "ok", "alpha ok", serde_json::json!({"k":"v"}), 42),
    );
    write_jsonl(&path, &body);

    let proj = project_tool_use_activity(&path).unwrap();
    let entry = proj[0].to_activity_entry(1_700_000_000);
    let detail = entry.detail.unwrap();
    assert!(detail.contains("summary: alpha ok"));
    assert!(detail.contains("duration_ms: 42"));
    assert!(detail.contains(&path.display().to_string()));
    assert!(
        !detail.contains("\"k\":\"v\""),
        "raw payload must not be in detail: {detail}"
    );
    assert_eq!(entry.title, "alpha ok");
}

// ---------------------------------------------------------------------
// 12. summary may contain text but raw payload + arguments stay redacted.
//
// D9 trusts envelope.summary (operator-written by the tool impl) but never
// renders envelope.payload or view.arguments, even when they contain data
// that looks like legitimate text.  This test ensures that the redaction
// boundary holds even when summary is non-trivial.
// ---------------------------------------------------------------------

#[test]
fn summary_can_contain_text_but_payload_arguments_still_redacted() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("summary_text.jsonl");
    // arguments contain a "secret_token" field that must never appear.
    // payload also contains a "secret_token" field that must never appear.
    // summary is deliberately verbose but safe.
    let args = serde_json::json!({"secret_token": "hunter2", "path": "/tmp/x"});
    let payload = serde_json::json!({"secret_token": "hunter2", "lines": ["a", "b"]});
    let body = format!(
        "{}\n{}\n",
        tool_call_line("s1", "read_file", args),
        tool_result_line("s1", "read_file", "ok", "read_file returned 2 lines", payload, 7),
    );
    write_jsonl(&path, &body);

    let proj = project_tool_use_activity(&path).unwrap();
    assert_eq!(proj.len(), 1);
    let p = &proj[0];

    // summary is forwarded verbatim — the tool impl owns it.
    assert_eq!(p.summary, "read_file returned 2 lines");

    // raw secrets must not appear anywhere in the projection struct.
    let rendered = format!("{:?} {:?}", p.summary, p.tool_name);
    assert!(!rendered.contains("hunter2"), "secret_token leaked into projection: {rendered}");

    // and not in the activity entry detail either.
    let entry = p.to_activity_entry(1_700_000_000);
    let detail = entry.detail.unwrap();
    assert!(!detail.contains("hunter2"), "secret_token leaked into activity detail: {detail}");
    assert!(detail.contains("read_file returned 2 lines"));
}
