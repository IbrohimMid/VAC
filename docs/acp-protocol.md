# ACP-over-Stdio Protocol (acp/1.0)

The Agent Control Protocol (ACP) provides a standard JSONL interface over standard input/output for external editor extensions or tools to communicate with the VacEngine. 

## Connection
The server reads `InboundEvent` JSON lines from `stdin` and writes `OutboundEvent` JSON lines to `stdout`.

## Message Types

### Client -> Server (InboundEvent)
```json
// Handshake (must be the first message)
{"event": "hello", "data": {"client": "my-editor-plugin", "protocol_version": 1}}

// Submit a prompt
{"event": "submit", "data": {"text": "write a python script to calculate fibonacci"}}

// Respond to an approval request
{"event": "permission_response", "data": {"request_id": "<uuid>", "allow": true, "reason": null}}

// Graceful detach
{"event": "detach"}
```

### Server -> Client (OutboundEvent)
```json
// Handshake response
{"event": "welcome", "data": {"session_id": "<uuid>"}}

// Streamed chunks of LLM output
{"event": "chunk", "data": {"text": "Here is the code..."}}

// Tool permission request
{"event": "permission_request", "data": {
    "request_id": "<uuid>", 
    "tool": "write_file", 
    "summary": "Write file main.py", 
    "arguments": {"path": "main.py"}
}}

// Task completion
{"event": "submit_finished"}

// Task abortion
{"event": "submit_aborted", "data": {"reason": "User cancelled"}}

// Error
{"event": "error", "data": {"reason": "Invalid JSON payload"}}
```