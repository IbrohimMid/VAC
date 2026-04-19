# Execute VAC Hourly Code Review (17:00 Run) Spec

## Why
Code review sweep (run 17:00) identified 7 active findings spanning core RAG/context retrieval, event-driven approval latency, memory leak in SHM, and various API/hygiene issues. Fixing these issues will prevent correctness failures, memory leaks, latency spikes, and potential integer overflow crashes.

## What Changes
- **F-01**: Refactor `ContextIndex::search` in `crates/vil_context/src/engine.rs` to use basic term-matching instead of ignoring the query.
- **F-02**: Migrate `wait_for_approval_intent` in `crates/vac_runtime/src/autopilot.rs` to use the event-driven `wait_for_record` API from `crates/vac_approvals/src/lib.rs` instead of 200ms polling.
- **F-03**: Add explicit eviction path `clear` and `evict` in `ContextEngine` (`crates/vil_context/src/engine.rs`) and call `shm.free` to prevent SHM memory leaks.
- **F-04**: Refactor `LlmRouter::complete` in `crates/vil_llm/src/router.rs` to use `next_retry_decision` from `retry.rs`.
- **F-05**: Remove `blocking_read()` from `ShmArena::as_ptr` in `crates/vil_context/src/shm.rs` to prevent tokio worker blocking. Add async version.
- **F-06**: Ensure `ContextIndex::add_entry` seeds scores using `entry.attention_weight` in `crates/vil_context/src/engine.rs`.
- **F-07**: Use `checked_add` in `ShmArena::{write, read}` guards in `crates/vil_context/src/shm.rs` to prevent integer overflows.

## Impact
- Affected specs: Core RAG, Autopilot execution, LLM routing, SHM access.
- Affected code: `crates/vil_context/src/engine.rs`, `crates/vil_context/src/shm.rs`, `crates/vac_runtime/src/autopilot.rs`, `crates/vac_approvals/src/lib.rs`, `crates/vil_llm/src/router.rs`.

## ADDED Requirements
### Requirement: Event-Driven Approval Intent
The system SHALL expose `wait_for_record` and a new `wait_for_intent` helper publicly in `vac_approvals` to be consumed by `vac_runtime::autopilot`.

### Requirement: SHM Memory Management
The system SHALL provide `clear` and `evict` APIs on `ContextEngine` to explicitly release SHM allocations.

## MODIFIED Requirements
### Requirement: Context Retrieval Relevance
The `ContextEngine::retrieve` SHALL use keyword overlap for scoring results rather than random HashMap iteration.

### Requirement: Retry Router
The LLM Router SHALL rely on `next_retry_decision` to enforce retry delays and max attempts logic.

### Requirement: SHM Access Safety
The `ShmArena` SHALL expose an async-safe mechanism to retrieve raw pointers or read data, avoiding `blocking_read`.

## REMOVED Requirements
N/A
