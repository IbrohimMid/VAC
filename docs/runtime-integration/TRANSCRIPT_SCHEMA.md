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

| Kind              | When written                                                                                  | `content` shape                                                                                  |
|-------------------|-----------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| `accepted`        | Synchronously, before the LLM is contacted. Durability checkpoint.                            | `{ "input": String, "submitted_at": RFC3339, "metadata": Value }`                                |
| `slash`           | A registered slash command handled the submit locally (no LLM round-trip).                    | `{ "command": String, "args": String, "summary": String, "payload": Value }`                     |
| `llm_request`     | Just before `LlmAdapter::complete` is called (LLM path only).                                 | `{ "prompt": String }`                                                                           |
| `llm_response`    | Immediately after the `LlmAdapter::complete` future resolves successfully.                    | `{ "provider": String, "model": String, "content": String, "input_tokens": u64, "output_tokens": u64 }` |
| `tool_call`       | **D7E** — once per tool the LLM requested, before gate / dispatch.                           | `{ "id": String, "name": String, "arguments": Value, "reason": Option<String>, "estimated_tokens": u64 }` |
| `tool_result`     | **D7E** — once per tool, after the envelope is built (success or error).                     | `{ "id": String, "name": String, "envelope": ToolResultEnvelope }`                               |
| `compact_boundary`| Compact boundary fired (regular path) or auto-compaction tripped before LLM contact.          | see "compact_boundary shapes" below                                                              |
| `finished`        | Final row on a clean submit.                                                                  | `{ "via": "llm" \| "slash", "usage": UsageSnapshot }`                                            |
| `aborted`         | Final row on a cancelled / errored submit.                                                    | `{ "reason": String, "kind": "budget_exceeded" \| "cancelled" \| "error" }`                      |
| `sidechain`       | Per subagent run (B.1). The subagent has its own transcript file.                             | `{ "subagent_id": Uuid, "subagent_type": String, "prompt": String, "result_summary": String }`   |

### `compact_boundary` shapes

`compact_boundary` is written from three places in `submit.rs`. The `content` shape varies by trigger:

```jsonc
// Regular drop_oldest hint (CompactHint::DropOldest { n }).
{ "kind": "drop_oldest", "n": usize }

// Regular summarise hint (CompactHint::Summarise { n, summary }).
{ "kind": "summarise", "n": usize, "summary": String }

// Auto-compaction preemption (D7E-era, A.4 path).
{ "trigger": "auto_compact", "used": u64, "ceiling": u64, "hint": String }
```

Parsers should treat `kind` and `trigger` as discriminants and tolerate either being absent — only one form is written per row.

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

## D8 note

## D9 note

D9 ships read-only transcript projection in
`vac_shell_host_transcript_projection`
(`project_tool_use_activity`, `summarize_tool_use`,
`session_tool_use_summary`). It consumes the existing D7E/D8
`tool_call` / `tool_result` rows — **no new TranscriptKind
variant is introduced**. The projection deliberately renders
only `tool name`, `status`, envelope `summary`, `duration_ms`,
and the transcript path; raw `envelope.payload` and original
`arguments` stay in the on-disk JSONL.

## D8 note

D8 wires real tool dispatch via
`vac_shell_host_vac_tool_dispatcher::VacToolDispatcher`. It does
**not** add a new `TranscriptKind` variant — the dispatcher's
output flows through the existing D7E `tool_result` row. The
envelope's `kind`/`payload`/`summary` are written verbatim by
the dispatcher, so transcripts produced under live dispatch
remain schema-compatible with transcripts produced under
`UnsupportedDispatcher`.

`vac_session_engine::read_tool_use_rows` is the canonical
helper for parsing these rows back; it pairs every `tool_call`
row with its matching `tool_result` row by id and is tolerant
of pre-D7E transcripts.

## Pinning tests

| Invariant                                          | Test                                                              | Crate                              |
|----------------------------------------------------|-------------------------------------------------------------------|------------------------------------|
| `accepted` is durable before LLM contact           | `accepted_row_durable_before_llm_contact`                         | `vac_session_engine`               |
| `tool_call` + `tool_result` rows persist           | `unsupported_dispatcher_writes_tool_call_and_tool_result_rows`    | `vac_session_engine`               |
| Multi-call order preserved                         | `multiple_tool_calls_preserve_transcript_order`                   | `vac_session_engine`               |
| Gate `Deny` writes error result, skips dispatcher  | `gate_deny_writes_error_tool_result_and_skips_dispatcher`         | `vac_session_engine`               |
| Dispatcher `Ok` writes ok result                   | `dispatcher_ok_writes_ok_tool_result_row`                         | `vac_session_engine`               |
| Tool rows visible from execute path (no event sink)| `d7e_tool_call_and_tool_result_rows_visible_via_execute_path`     | `vac_shell_host_vac_command_adapter` |
