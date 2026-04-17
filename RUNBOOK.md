# VAC Runbook

This document contains standard operating procedures (SOP) and recipes for the Vastar Agentic CLI.

## Recipes

### 1. Extracting JSON Logs
When running the agent in automation or CI/CD environments, use JSON formatting for easy parsing by `jq` or observability tools:
```bash
vac --log-format json run "refactor auth logic" | jq .
```

### 2. Monitoring Performance with OpenTelemetry
Use Jaeger or another OTLP-compatible backend:
```bash
export VAC_OTEL_ENDPOINT="http://localhost:4317"
vac run "build the project"
```

### 3. Scraping Metrics with Prometheus
To expose metrics on port 9000:
```bash
vac --metrics-addr 0.0.0.0:9000 run "analyze codebase"
```

## Failure Modes

### 1. Crash Dump Handling
**Symptom:** VAC panics and drops a `crash_dump.json`.
**Resolution:**
1. Inspect the `crash_dump.json` for the exact panic payload and location.
2. If it points to an invalid state, reset the session (`vac restore <file>`).
3. Report the panic trace in an issue if it's a runtime bug.

### 2. Agent Tool Timeout
**Symptom:** "Command timed out after X seconds"
**Resolution:**
- Check if the command was waiting for user input.
- Check if the command was stuck in an infinite loop.
- Override `timeout_secs` in the rulebook or policy configuration if the operation is known to take longer.

### 3. Memory/Disk Quota Exhaustion
**Symptom:** Process killed with `SIGKILL` or "No space left on device" (Disk Quota Exceeded).
**Resolution:**
- The command attempted to use more than 2GB RAM or 1GB disk space.
- Optimize the script to use streaming instead of buffering in memory.
- If expected, request a quota increase in the system configuration.
