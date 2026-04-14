# Structured Approval Flow Migration

## Current State (Natural Language)

Currently, approval/rejection is sent as natural language messages to the LLM:
- Accept: "I have APPROVED the tool call '{}' with arguments '{}'. Please proceed."
- Reject: "I have REJECTED the tool call '{}'. Please revise your plan."

This is marked as TECHNICAL DEBT in `runner.rs`.

## Target State (Structured Flow)

Should use ID-based approval system:

```rust
// Instead of natural language message
OutputEvent::AcceptTool(tool_call) => {
    // Send structured approval with tool_call_id
    engine.approve_tool_call(tool_call.id).await?;
}

OutputEvent::RejectTool(tool_call, reason) => {
    // Send structured rejection with tool_call_id
    engine.reject_tool_call(tool_call.id, reason).await?;
}
```

## Implementation Steps

1. Add `approve_tool_call()` and `reject_tool_call()` methods to VacEngine
2. Update RuntimeUpdate to include ApprovalResponse variant
3. Modify runner.rs to use structured calls instead of natural language
4. Update event flow to handle approval responses

## Benefits

- More reliable than natural language parsing
- Clearer intent
- Better error handling
- Easier to track approval state
