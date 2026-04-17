# Runbook

Operational notes for the main subsystems.

## Engine init fails

- Run `vac doctor --strict --format json` to inspect the failing subsystem.
- Confirm `.vac/config.toml` exists and the `[llm]` section loads cleanly.
- Check the latest session and checkpoint files under `.vac/sessions/` and
  `.vac/checkpoints/`.

## LLM provider unreachable

- Run `vac doctor`.
- Confirm the provider-specific environment variables are present.
- Inspect [docs/PROVIDER_PARITY.md](./PROVIDER_PARITY.md) for reload behavior
  and the smoke matrix. Provider wiring can be refreshed by
  `VacEngine::reload_config()`; restart only if your deployment does not expose
  that path.

## Scheduler appears stuck

- Run `vac runtime status`.
- Inspect `vac runtime jobs --format json`.
- Check the queue files under `.vac/queue.json` and `.vac/agent_queue.json`.

## MCP server down

- Run `vac mcp status`.
- Confirm the configured server endpoints are reachable.
- Check the trust classification in `.vac/config.toml`.

## Bundle import failed

- Re-run with `--format bundle-json` and `--require-signed` if the bundle should
  be verified.
- Inspect the bundle path and size caps.
- Check [docs/THREAT_MODEL.md](./THREAT_MODEL.md) for the import boundary.

## Disk full

- Check the `.vac/` directory size first.
- Delete old exports and checkpoints after confirming they are no longer
  needed.
- Recover free space before retrying writes.

## OOM kill

- Reduce the concurrency of the active task.
- Prefer smaller work chunks and tighter tool timeouts.
- Confirm the host memory cap or container limit is adequate.
