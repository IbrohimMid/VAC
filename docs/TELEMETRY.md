# VAC Telemetry & Observability

This document details the telemetry and observability features built into the Vastar Agentic CLI.

## Structured Logs
VAC supports JSON structured logging. Use `--log-format json` to output JSON instead of text. This is useful for shipping logs to an aggregator like Fluentd, Datadog, or Elasticsearch.

## OpenTelemetry
You can export traces via OTLP (gRPC) using the `--otel-endpoint` flag or `VAC_OTEL_ENDPOINT` environment variable.
Example:
```bash
vac --otel-endpoint http://localhost:4317 run "my task"
```

## Metrics
Prometheus metrics are exposed when `--metrics-addr` is provided. Counters and histograms track tool invocations, durations, and errors.
Example:
```bash
vac --metrics-addr 127.0.0.1:9000 run "my task"
```

## Crash Capture
In the event of a panic, a structured JSON crash dump is output to stderr and saved to `crash_dump.json` in the current working directory. This helps in post-mortem debugging.
