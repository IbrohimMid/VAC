# VAC Agent Journal

> Jurnal ini digunakan untuk melacak progres eksekusi ultraplan (M1..M14 + P1..P3).
> Format setiap entri milestone:
>
> ## M<N> <name> — <status: DONE | SKIPPED | BLOCKED>
> Commit: <hash>
> Tests: <names>
> Evidence: <ultraplan §3/§4 row + line number>
> Notes: <any blockers, flakes, follow-ups>
>
> Di akhir setiap wave, tambahkan blok `## Wave N summary` dengan:
> - Milestones done vs skipped
> - Aggregate test count
> - Adoption-score delta

---

## M1 Boot phase split + profile — DONE
Commit: 0fbdb42020eb3929fa5f20bc19d6754ae7f1e730
Tests: cargo check -p vac_cli --tests
Evidence: M1 in ultraplan §3
Notes: Implemented BootProfile, wrapped startup tasks into Critical and Deferred blocks.

## M8 App shell ≤ 20 flat — DONE
Commit: 8031843a3370e610d3dfb7f7db86cb0add9150f6
Tests: cargo check --all-targets
Evidence: M8 AppState flat-field census ≤ 20
Notes: Refactored `AppState` into 9 domains (core, layout, composer, transcript, session, workspace, vil_domain, execution, operator_config). Replaced all accessors across `vac_tui_runtime` and tests.

## M10 Ingest BM25 persistence — DONE
Commit: 36f6ef36ee0fddbb26ff697c677413f22b543503
Tests: cargo check -p vac_ingest -p vac_cli -p vac_tui_runtime --tests
Evidence: Ingest command reads/writes `~/.vac/bm25.index` instead of memory-only.
Notes: Created `vac ingest` command, binary serialisation for `Bm25Index`, and integrated `FileIndexReady` with `bm25_index` Option in `vac_tui_runtime`.

## M2 Engine convergence — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo nextest run --workspace
Evidence: M2 in ultraplan §3
Notes: Removed EngineMode::Legacy, fixed compilation errors and e2e test usages of vac autopilot up.

## M2.1 Budget gate + orphan track — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo nextest run -p vac_session_engine -p vac_cli
Evidence: M2.1 in ultraplan §3
Notes: Implemented budget gate check in `submit_one` with typed error `BudgetExceeded`. Plumbed `--budget-tokens` to `vac run`. Tests updated and passed.

## M2.2 File history by submit ID — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo nextest run --workspace
Evidence: M2.2 in ultraplan §3
Notes: Extended BackupRecord with submit_id. FileWriteTool/FileEditTool plumb it through ToolContext. `vac restore` now accepts `--submit <uuid>` to reverse all changes in a submit. Fixed a flaky test in autopilot by using unique tmp paths.

## M9 Resume e2e — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo nextest run --test m9_resume
Evidence: M9 in ultraplan §3
Notes: TUI boot detects pending submit via `last_pending_submit`, pushes `AskUser` overlay, and resumes using the stored `Accepted` row. Bumped `SessionSnapshot::schema_version` to 2.

## M3 Tool spec() explicit — DONE
Commit: 3fb9cc9a9040ffcde7bdfd92d42d3c7ed303ce96
Tests: cargo check -p vac_tools --tests
Evidence: M3 in ultraplan §3
Notes: Removed default body for `VilTool::spec` and explicitly implemented it in all 37 built-in tools. Added audit test.

## M3.1 ToolSpec richness — DONE
Commit: 3fb9cc9a9040ffcde7bdfd92d42d3c7ed303ce96
Tests: cargo check -p vac_tools --tests
Evidence: M3.1 in ultraplan §3
Notes: Added `prepare_permission_matcher`, `interrupt_behavior`, `inputs_equivalent`, `search_read_classification` to `VilTool` trait with default implementations. Overrode them in `file_write`, `task_create`, `bash`, and `search` tools.

## M3.2 ToolResultEnvelope round-trip — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo check --workspace --tests
Evidence: M3.2 in ultraplan §3
Notes: Changed `SubmitEvent::ToolResult` to hold `payload: ToolResultEnvelope` instead of raw strings. Updated `RuntimeUpdate::ToolResult` to pipe it through to the TUI. Modified TUI `ToolCallResult` to hold `envelope` and `render_tool_result` to use `envelope.summary` when present. Added `PartialEq` to `ToolResultEnvelope` and `ToolResultKind`.

## M5 MCP primary swap — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo check --workspace --tests
Evidence: M5 in ultraplan §3
Notes: Replaced legacy `vac_tools::mcp::McpConnectionState` with canonical `vac_mcp_core::McpConnection` in `AppState::mcp_maps`. Updated TUI event loop (`McpServerState` event) and rendering (`side_panel.rs`, `workbench/runtime.rs`) to match the new 5-state machine. Removed `trust_class` and mode checks from the UI (deferred to config retrieval).

## M5.1 Add WebSocket + HTTP transports — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo check --workspace --tests
Evidence: M5.1 in ultraplan §3
Notes: Added `WebSocket` variant to `vac_tools::mcp::McpTransport` and `McpConnection`. Implemented JSON-RPC over WebSocket using `tokio-tungstenite`. `Sse` transport with `reqwest` was already present.

## M4 TrustGate unified entry point — DONE
Commit: $(git rev-parse HEAD)
Tests: cargo check --workspace --tests
Evidence: M4 in ultraplan §3
Notes: Created `vac_tools::trust_gate::TrustGate` (re-exported in `vac_approvals::trust_gate`) as the unified entry point. Integrated `TrustGate::check_tool` into `vac_tools::ToolRouter` to override or respect legacy policy decisions based on environment constraints and MCP trust configurations.
