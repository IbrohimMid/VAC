# Runtime Jobs Guide

## Overview

VAC runtime manages background jobs for scheduled tasks, file watching, and one-shot operations.

## Job Types

### OneShot
Single execution task.
- **Use when:** Manual task execution
- **Trigger:** Immediate
- **Retry:** Configurable

### Cron
Scheduled recurring task.
- **Use when:** Periodic maintenance
- **Trigger:** Cron expression
- **Retry:** Per execution

### FileWatch
File change triggered task.
- **Use when:** Auto-rebuild, hot reload
- **Trigger:** File modification
- **Retry:** Per trigger

## Job Kinds

### RunTask
Execute agent task with description.
```rust
JobKind::RunTask { description: "Fix linting errors" }
```

### DiagnosticSweep
Run diagnostic checks across codebase.
```rust
JobKind::DiagnosticSweep
```

### RulebookComplianceCheck
Verify rulebook compliance.
```rust
JobKind::RulebookComplianceCheck
```

### PatchProposal
Generate patch for specific files.
```rust
JobKind::PatchProposal { files: vec!["src/main.rs"] }
```

### ToolCall
Execute specific tool.
```rust
JobKind::ToolCall { tool_name: "lint", arguments: json!({}) }
```

## Job States

- **Queued** - Waiting for execution
- **Running** - Currently executing
- **Completed** - Finished successfully
- **Failed** - Execution failed
- **Cancelled** - Manually cancelled

## CLI Commands

### List jobs
```bash
vac runtime jobs
```

### Check runtime status
```bash
vac runtime status
```

### Cancel job
```bash
vac runtime cancel <job-id>
```

### Retry failed job
```bash
vac runtime retry <job-id>
```

### Inspect job details
```bash
vac runtime inspect <job-id>
```

## TUI Runtime Tab

Navigate to Runtime tab (press `4` in Workbench):

### Job List
- Shows all jobs with status
- `j`/`k` to navigate
- `Enter` to view details

### Job Actions
- `c` - Cancel selected job
- `t` - Retry selected job
- `r` - Refresh job list

### Job Details
- Job ID and kind
- Status and timestamps
- Retry count
- Result summary
- Error messages (if failed)

## Retry Policy

Jobs automatically retry on failure:
- Default: 3 retries
- Exponential backoff
- Configurable per job

## Best Practices

1. **Monitor job status** - Check Runtime tab regularly
2. **Cancel stuck jobs** - Don't let failed jobs accumulate
3. **Retry transient failures** - Network errors, timeouts
4. **Review failed jobs** - Check error messages
5. **Limit concurrent jobs** - Avoid resource exhaustion

## Autopilot Integration

Runtime jobs integrate with autopilot daemon:

```bash
# Start autopilot
vac autopilot up

# Check status
vac autopilot status

# Stop autopilot
vac autopilot down
```

Autopilot manages:
- Job queue processing
- Cron scheduling
- File watch monitoring
- Retry logic
- State persistence

## Job Lifecycle

1. **Enqueue** - Job added to queue
2. **Dequeue** - Job picked up for execution
3. **Execute** - Job runs with timeout
4. **Complete/Fail** - Result recorded
5. **Retry** - If failed and retries remaining
6. **Archive** - Final state persisted

## Troubleshooting

**Job stuck in Running:**
- Cancel and retry
- Check autopilot logs
- Verify timeout configuration

**Job fails immediately:**
- Check job configuration
- Verify execution environment
- Review error message

**Jobs not processing:**
- Ensure autopilot is running: `vac autopilot status`
- Check queue: `vac runtime jobs`
- Review runtime logs

**High retry count:**
- Investigate root cause
- Fix underlying issue
- Cancel and recreate job
