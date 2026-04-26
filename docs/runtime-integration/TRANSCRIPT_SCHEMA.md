# Transcript schema (`.vac/sessions/<session_id>.jsonl`)

> **Source of truth**: `vac_session_engine::transcript::TranscriptEntry`
> + `TranscriptKind`. This file is human-readable documentation —
> when the engine and this doc disagree, the engine wins. Update
> this file in the same commit that adds or removes a `TranscriptKind`
> variant.

Each line in the JSONL is one `TranscriptEntry` value:

```json
{
  "id":         "<uuid>",            // entry id, unique per row
  "session_id": "<uuid>",            // submit session id
  "kind":       "<snake_case kind>", // see table below
  "timestamp":  "<rfc3339 utc>",     // when the row was appended
  "content":    { ... }              // kind-specific payload
}
```

`content` is `serde_json::Value`; its shape depends on `kind`. Rows
are appended with per-row `fsync` so a crash at any point leaves a
self-consistent prefix on disk.

## Row kinds

| Kind              | When written                                                                | `content` shape                                                                                  |
|-------------------|-----------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `accepted`        | Synchronously, before the LLM is contacted. Durability checkpoint.          | `{ "input": String, "submitted_at": RFC3339, "metadata": Value }`                                |
| `slash`           | A registered slash command handled the submit locally (no LLM round-trip). | `{ "command": String, "payload": Value }`                                                        |
| `llm_request`     | (Reserved.) Currently not appended in the dogfood path; future engines may. | implementation-defined                                                                           |
| `llm_response`    | Immediately after the `LlmAdapter::complete` future resolves successfully.  | `{ "provider": String, "model": String, "content": String, "input_tokens": u64, "output_tokens": u64 }` |
| `tool_call`       | **D7E** — once per tool the LLM requested, before gate / dispatch.          | `{ "id": String, "name": String, "arguments": Value, "reason": Option<String>, "estimated_tokens": u64 }` |
| `tool_result`     | **D7E** — once per tool, after the envelope is built (success or error).   | `{ "id": String, "name": String, "envelope": ToolResultEnvelope }`                               |
| `compact_boundary`| Compact boundary fired before contacting the LLM.                           | `{ "kept": usize, "dropped": usize, "hint": Value }`                                             |
| `finished`        | Final row on a clean submit.                                                | `{ "via": "llm" \| "slash", "usage": UsageSnapshot }`                                            |
| `aborted`         | Final row on a cancelled / errored submit.                                  | `{ "reason": String }`                                                                           |
| `sidechain`       | Per subagent run (B.1). The subagent has its own transcript file.           | `{ "subagent_id": Uuid, "subagent_type": String, "prompt": String, "result_summary": String }`   |

`ToolResultEnvelope` (from `vac_tool_core`):

```json
{
  "kind":        "ok" | "warning" | "error" | "cancelled",
  "payload":     Value,
  "summary":     String,
  "duration_ms": u64
}
```

## Ordering invariants

- `accepted` is the first row of every submit.
- For each tool the LLM requested:
  - `tool_call` appears before its corresponding `tool_result`.
  - `tool_result` appears before `finished`.
  - For multiple tools, `tool_call` order matches provider order; the
    matching `tool_result` rows follow in the same order.
- `finished` (or `aborted`) is the last row of every submit.

## Backwards-compatibility policy

`TranscriptKind` is `#[non_exhaustive]`. Adding new variants is a
non-breaking change for parsers that match exhaustively only on the
kinds they care about. Removing or renaming a variant requires a
migration note in this file plus a transcript-replay test.

## Pinning tests

| Invariant                                          | Test                                                              | Crate                              |
|----------------------------------------------------|-------------------------------------------------------------------|------------------------------------|
| `accepted` is durable before LLM contact           | `accepted_row_durable_before_llm_contact`                         | `vac_session_engine`               |
| `tool_call` + `tool_result` rows persist           | `unsupported_dispatcher_writes_tool_call_and_tool_result_rows`    | `vac_session_engine`               |
| Multi-call order preserved                         | `multiple_tool_calls_preserve_transcript_order`                   | `vac_session_engine`               |
| Gate `Deny` writes error result, skips dispatcher  | `gate_deny_writes_error_tool_result_and_skips_dispatcher`         | `vac_session_engine`               |
| Dispatcher `Ok` writes ok result                   | `dispatcher_ok_writes_ok_tool_result_row`                         | `vac_session_engine`               |
| Tool rows visible from execute path (no event sink)| `d7e_tool_call_and_tool_result_rows_visible_via_execute_path`     | `vac_shell_host_vac_command_adapter` |
