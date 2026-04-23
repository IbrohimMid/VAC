# VAC Completion Blueprint — Closing the Implementation-Plan Gaps

> ⚫ **Superseded.** The 7-ring convergence plan below has been
> absorbed into [`ultraplan-vac-product.md`](ultraplan-vac-product.md)
> §3 milestone table. The ledger in §8 below remains a useful
> historical record of what shipped in the R0..R6 autonomous run.
> For current execution plans, start with
> [`ROADMAP.md`](ROADMAP.md).

**Status:** partial — see §8 for the landed-vs-deferred ledger.
**Updated:** 2026-04-23
**Scope:** convergence plan from `docs/implementation-plan.md` Fase 0–10 to
production-ready VAC.
**Non-goal:** new product surface. This document does NOT add features
beyond what the original plan promised. It's a sequencing plan for
finishing the parts landed as API-only, scaffold, or dead-code.

---

## 1. Honest state (audit snapshot)

Every phase was audited in sequence with tests + push. The result is
architecturally clean but tactically half-wired. Five strategic crates
exist; only two are on the critical path:

| Crate | Landing fase | Wired to production path? |
|---|---|---|
| `vac_tool_core` | F1 | **Yes** — `spec()` default on `VilTool` drives the registry. |
| `vac_session_engine` | F2 | **No** — `vac run` still calls `vac_core::VacEngine`; `submit_one` only driven by `vac session-run`. |
| `vac_memory` | F4 | **Partially** — consolidator + banner present; `vil_memory` struct-field pattern never removed. |
| `vac_mcp_core` | F5 | **No** — `vac_tools::mcp::core` re-export is additive; legacy `McpConnectionState` is still the consumer surface. |
| `vac_bridge` | F5 | **No** — `vac acp serve` still uses `vac_core::AcpServer`. |

Nine UX services (F7) are pure logic with unit tests but only two
(`memory_banner`, `memory_consolidator`) are referenced from `AppState`
or the view layer. The other seven are shelf code.

Single-number summary: roughly 68 % of the plan is implemented; 95 %
of the classes/types/tests the plan required exist; ~30 % of the
production paths those types were meant to replace actually flow
through them.

---

## 2. Gap inventory by severity

### Critical (plan promised; production path blocked)

| Id | Gap | Impact |
|---|---|---|
| **G1** | `vac run` ignores `submit_one`. Two parallel engines. | Session resumption contract lives only in `session-run`. |
| **G2** | `AppState` still carries ~40 grandfathered flat fields. | Domain regrouping goal (F3.4) is aspirational. |
| **G3** | `FileEditTool` / `FileWriteTool` never call `backup::snapshot_file`. | Reversible-edit promise (F6.4) is dead code. |
| **G4** | `vil_memory` struct-field `WorkingMemory`/`EpisodicMemory` untouched. | Two parallel memory systems, no convergence. |
| **G5** | MCP status still routes through `vac_tools::mcp::McpConnectionState`. | `vac_mcp_core` state machine is not the source of truth. |
| **G6** | Bulk approval is state-only. No checkbox renderer, no keybind. | F6.2 is half-built. |

### High (feature works in isolation; no real users)

| Id | Gap | Impact |
|---|---|---|
| **G7** | `vac_ingest::bm25::rank_paths` has zero call sites. | File picker / @mention don't use BM25. |
| **G8** | `vac acp serve` never touches `vac_bridge::AcpServer`. | Bridge crate is island; Fase 5 integration test is the only exercise. |
| **G9** | Services (`notifier`, `prevent_sleep`, `token_estimation`, `prompt_suggest`, `rate_limit`, `sparkline`, `hunk_fold`, `rewind`) sit in `crates/vac_tui_runtime/src/services/` with no AppState / view hookup. | Seven pure-logic modules operators cannot actually reach. |
| **G10** | Trace `AgentDecision` (F1.1 leftover) surfaced via CLI but not in TUI. | Trajectory evaluation only reachable headless. |

### Medium (scope noted in plan, explicitly deferred)

| Id | Gap | Original note |
|---|---|---|
| **G11** | `vil_inference::backends::candle` is a scaffold; `MockBackend` is the only usable backend. | Paket E / B1 deliverable. Decision pending on FFI toolchain. |
| **G12** | `vil_rag` HNSW feature-gated; `LinearAnnIndex` is the default. Persistence layout unresolved. | Paket C / B2. |
| **G13** | `vac_tools::rust_analysis` is trait + stub. No portable-pty integration. | Paket E / M1. Blocked on `ra_ap_*` API stability. |
| **G14** | `vac_ux_services` extraction — seven UI-agnostic services inside `vac_tui_runtime` drag ratatui into any headless consumer. | Fase 7 audit defer. |

### Low (docs + benches)

| Id | Gap | Impact |
|---|---|---|
| **G15** | No `.github/workflows` link-check; deployment.md claims were lying pre-audit. | Doc drift risk. |
| **G16** | No criterion benches under `benches/`; only SLA assertion tests. | No statistical bench pipeline. |
| **G17** | `scripts/check_layering.sh` only enforces `vil_* → vac_*`. L1–L5 layering is convention. | Architecture doc called out. |

---

## 3. Blueprint — ordered by dependency, not importance

Integration debt has a shape: **the outer rings can be closed only
after the inner ring converges**. Attacking G7 before G1 just adds
more consumers to a path that's about to be retired.

### Ring 0 — Converge the engine (G1)

**Why first:** every other "real user" gap assumes one engine, not
two.

- **0a.** Extract `vac_core::engine::RuntimeUpdate` → events compatible
  with `vac_session_engine::SubmitEvent`. Add an adapter layer in
  `vac_session_engine::bridge_vacengine` that calls submit_one with
  a `VacEngineAdapter: LlmAdapter`.
- **0b.** Rewrite `vac_cli::commands::run::execute` to call
  `submit_one` with the `VacEngineAdapter`. Keep the existing stdout
  UX (status/tool-call/approval prompts) by translating SubmitEvent
  → the existing `println!` block.
- **0c.** Rewrite `vac_tui_runtime::runner` entry to drive submit_one
  for each user message instead of `VacEngine::run_task_with_approvals`.
- **0d.** Retire `vac session-run` (or keep as an alias for
  `vac run --provider mock`).
- **Contract tests:** move the existing `provider_matrix.rs` tests to
  run via both paths; both must produce identical terminal
  transcripts.

**Effort:** 2–3 d. High-risk; every integration test in the repo
touches this surface.

### Ring 1 — Flatten AppState (G2)

**Why second:** once the engine is one, the state seam is the next
thing every driver reads.

- **1a.** `BillingState { token_usage, total_usage, context_pct, billing_info, auth_display }` — 5 fields → 1.
- **1b.** `McpMapsState { server_states, signals, runtime_signals }` — 3 fields → 1.
- **1c.** `ImageRenderState { pending_kitty, last_kitty, preview_cache }` — 3 fields → 1.
- **1d.** `ThemeUi { theme }` merge into `OperatorState` (F3.1).
- **1e.** Flat field count target: <25 (realistic; plan's <20 was
  aspirational).
- **1f.** Contract test: `appstate_flat_field_census_stays_below_25`
  using reflection via `std::mem::size_of` estimates.

**Effort:** 1–2 d. Low risk; all mechanical sed.

### Ring 2 — Wire the already-built state (G3, G6, G9 partial)

**Why third:** builds on converged engine + clean state.

- **2a. G3** — `FileEditTool::execute` and `FileWriteTool::execute`
  call `vac_tools::backup::snapshot_file(project_root, path)` before
  the write. Operator sees a backup id in the approval dialog.
- **2b. G6** — Bulk approval keybind (`Space` toggles, `A` applies
  approve, `R` applies reject). View layer renders a `[x]` column
  to the left of each pending row. Drain through the existing per-call
  approval channel.
- **2c. G9 partial** — Wire three of the seven orphan services:
  - `notifier::TracingNotifier` on task finish + build finish.
  - `token_estimation` footer pip (Safe/Warn/Danger) next to composer.
  - `rate_limit::RateLimitState` on throttle events from
    `vil_llm::error`.
- **2d. Keep deferred:** `prevent_sleep`, `prompt_suggest`, `sparkline`,
  `hunk_fold`, `rewind` — these want larger UX design work and are
  low-value for the current operator profile.

**Effort:** 2 d total. Each sub-task self-contained.

### Ring 3 — Consolidate subsystems (G4, G5, G8)

**Why fourth:** can touch `vil_memory` / `vac_tools::mcp` only after
every caller transitively sits on top of the converged spine.

- **3a. G4 vil_memory retirement:**
  - Identify callers of `WorkingMemory::append`, `EpisodicMemory::recall`
    etc. (vil_swarm + vac_core).
  - Replace with `vac_memory::MemoryScanner + query::find_relevant`.
  - Delete `vil_memory` from workspace once zero callers remain.
  - Keep `vil_memory` crate only if another VIL subsystem depends on
    the redb tiered layout (today: none).
- **3b. G5 mcp primary swap:**
  - Rewrite `vac_tools::mcp::probe_mcp_server` to return
    `vac_mcp_core::McpConnection` directly (via
    `StateTransition::ConnectOk / AuthNeeded / ConnectFail`).
  - Update `AppState.mcp_server_states` (now `mcp_maps.server_states`)
    to hold `McpConnection` — legacy `McpConnectionStatus` removed.
  - Status panel renders the 5-state machine's labels.
- **3c. G8 ACP primary swap:**
  - Replace `vac_core::AcpServer` inside `vac_cli/src/commands/acp.rs`
    with `vac_bridge::AcpServer`.
  - The existing approval-handler callback becomes a
    `PermissionMediator` impl.
  - The submit loop inside `vac acp serve` drives `submit_one` via
    the Ring 0 `VacEngineAdapter`.

**Effort:** 3–4 d. Deletion-heavy; every migration needs regression
test coverage.

### Ring 4 — Adopt dead code (G7, G9 remainder)

- **4a. G7 BM25 wiring:**
  - `vac_tui_runtime::services::file_search::fuzzy_search_files` adds
    a BM25 pass over the file index when the input has ≥3 chars.
  - `@mention` resolver reads BM25 top-k.
  - `vac_ingest::ProjectContext` exposes `bm25_hint(query, k)` for
    downstream services.
- **4b. G9 remainder:** only if an operator asks for it.
  `prompt_suggest` is the most valuable next wire; it belongs in the
  composer's bottom footer. `sparkline` pairs with a streaming tok/s
  telemetry channel that doesn't yet exist — wait until then.

**Effort:** 1 d for G7; G9 remainder is elastic.

### Ring 5 — Scaffolds with real work

- **5a. G11 Candle:** decide on `candle-core` GGUF loader vs a `wgpu`
  backend; write a single-prompt Phi-3-mini integration test gated
  behind `--features candle`. Stop at "one model loads and produces
  a plausible token stream". Full inference stack = future fase.
- **5b. G12 HNSW:** swap `LinearAnnIndex` for
  `instant-distance::HnswMap` inside `vil_rag::index::RagIndex` when
  the corpus crosses 10 k embeddings. Persist the graph to
  `.vac/rag/index.bin` via `bincode`.
- **5c. G13 rust_analysis:** spawn `rust-analyzer` via
  `portable-pty`, send `initialize` + a couple of `textDocument/*`
  requests, parse diagnostics. Gate behind a feature until the
  `ra_ap_*` crate pins.

**Effort:** 5a: 3–4 d. 5b: 2 d. 5c: 3–4 d.

### Ring 6 — Packaging

- **6a. G14** — extract seven UI-agnostic services to
  `crates/vac_ux_services`. Move tests as-is. Update
  `vac_tui_runtime` re-exports so call sites don't churn.
- **6b. G16** — add `benches/primary_action.rs` criterion bench
  (already have `bench(tui): O2 criterion primary_action harness`
  commit; wire into the SLA track).
- **6c. G17** — tighten `scripts/check_layering.sh` to reject any new
  cross-layer edge noted in the `L1..L5` comment in
  `architecture.md`. Optional.
- **6d. G15** — add `lychee` doc-link check to CI.

**Effort:** 2 d total.

---

## 4. Effort + slice summary

| Slice | Rings | Effort | Delivers |
|---|---|---|---|
| **Convergence** | 0, 1, 2 | 5–7 d | `submit_one` is the spine; bulk approve works; backups hook in; Op footers live. |
| **Subsystem cleanup** | 3 | 3–4 d | One memory, one MCP, one ACP. Delete dead legacy surfaces. |
| **Feature adoption** | 4 | 1–2 d | BM25 actually ranks files; orphan services reach operators. |
| **Scaffold completion** | 5 | 8–10 d | Candle/HNSW/rust-analysis move from "defined" to "works once". |
| **Packaging** | 6 | 2 d | `vac_ux_services`, link-check CI, criterion benches, layer gate. |

Total to close the plan: **19–25 engineering-days** for one senior
engineer, excluding review cycles.

---

## 5. Cut lines

If the team has to pick:

- **Must** (closes the contract): Rings 0, 1, 2. Without these, the
  "Resumable", "Truthful state", and "Reviewable autonomy" value
  props in PRODUCT_SPEC.md are half-delivered.
- **Should**: Ring 3. Without these, dev-time is fine but the
  architecture document lies about what's load-bearing.
- **Nice**: Ring 4, 5, 6. Every one of these can ship in its own
  cycle without blocking the others.

---

## 6. Risks + mitigations

1. **Engine convergence breaks TUI.** Many call sites read
   `VacEngine::approval_handle()` / `run_task_with_approvals`.
   Mitigation: cutover is feature-flagged (`VAC_ENGINE=session`) for
   two weeks; both paths coexist under the flag, retire the old one
   once the TUI integration tests pass on the new path.
2. **vil_memory consumers in vil_swarm.** Removing the struct fields
   cascades into the planner/executor agents. Mitigation: introduce a
   `vil_memory::adapter::VacMemoryBridge` first; vil_swarm calls that
   bridge; flip implementation in one commit.
3. **MCP swap surfaces in external consumers.** Any downstream tool
   that imports `vac_tools::mcp::McpConnectionState`. Mitigation: grep
   all sites; move is sed-level; tests are the safety net.
4. **rust-analyzer API churn.** Pin a specific `ra_ap_*` revision and
   commit the Cargo.lock. Review quarterly.

---

## 7. Tracking

Each ring maps to a Fase-style tracked issue. Commit messages use the
same `refactor(area): R<N>.<sub> — <short>` discipline as the audit
cycles so a future architect can diff rings exactly like fases.

**Post-closure state:** the codebase is the same ~31 crates, but the
data flow matches the architecture diagram. Every value proposition
in `PRODUCT_SPEC.md` is load-bearing on tests that the CI would
actually run — not on API shapes nobody consumes.

---

## 8. Landed vs deferred ledger (2026-04-23)

Honest accounting of what shipped in this cycle versus what remains
open. Ring IDs map to commit prefixes so `git log --grep='R0.a'`
surfaces the exact landing.

### Landed

| Ring | Commit | Summary |
|---|---|---|
| **R0.a** | 2445c0a | `VacEngineAdapter` wraps VacEngine as `LlmAdapter`; translates `RuntimeUpdate` ↔ `SubmitEvent`. |
| **R0.b** | 2445c0a | `vac run --engine session` + `VAC_ENGINE=session` env fallback; legacy path default. |
| **R0.c** (detection) | d7c1d2e | `vac_tui_runtime::runner` detects `VAC_ENGINE=session` and logs the intent. Full migration still requires the 2-week coexistence window. |
| **R0.d** | e36ab83 | `vac session-run` docstring redirects to `vac run --engine session` for real VacEngine; kept as mock-only alias candidate. |
| **R1.a** | e36ab83 | `BillingState` groups token_usage + context_pct + billing_info + auth_display (5 fields → 1). |
| **R1.b** | e36ab83 | `McpMapsState` groups the three MCP+runtime HashMaps. |
| **R1.c** | e36ab83 | `ImageRenderState` groups the three Kitty-graphics fields. |
| **R2.a** | 1f0d897 | `FileEditTool` + `FileWriteTool` call `vac_tools::backup::snapshot_file` before writes; complementary to the session journal. |
| **R2.b** (handlers) | d7c1d2e | `handlers::approval::{bulk_toggle_current, bulk_clear, bulk_approve, bulk_reject}`. View checkbox renderer + keybind still pending. |
| **R2.c** (state) | d7c1d2e | `AppState.rate_limit` + `AppState.prompt_history` first-class fields; contract test locks reachability. Handler wire-up on submit / throttle events still pending. |
| **R3** (bridge) | d7c1d2e | `vac_tools::mcp::McpConnectionState::to_core_connection()` emits the canonical `vac_mcp_core::McpConnection`; legacy primary still in place. |
| **R4** | 1f0d897 | `ranked_search_files` in `services::file_search` blends BM25 (≥3 char queries) with the existing nucleo fuzzy path. |
| **R5.b** (linear path) | current | `vil_rag::RagIndex::search_via_linear` routes cosine scan through `LinearAnnIndex`; HNSW swap is a one-line flip once persistence lands. |
| **R6.a** (link-check) | 4e70d19 | `.github/workflows/doc-links.yml` lychee non-blocking link-check on docs PRs. |
| **R6.b** (bench) | 4e70d19 | `crates/vac_session_engine/benches/submit_one.rs` criterion statistical bench complements the SLA tests. |

### Deferred (require multi-day work or coexistence window)

| Ring | Why not yet |
|---|---|
| **R0.c** (full migration) | Blueprint §6 Risk 1 mandates a 2-week `VAC_ENGINE=session` coexistence window before retiring the legacy TUI path. Detection stub landed; full rewrite is a follow-up cycle. |
| **R2.b** (view + keybind) | Checkbox column renderer + `Space` toggle + `Ctrl+Shift+A`/`Ctrl+Shift+R` bulk keybinds need an overlay design review. Handlers + state ready. |
| **R2.c** (submit/throttle wire) | Recording prompts on submit + cycling rate-limit messages on 429 events needs the engine-event bus; waiting on R0.c full migration. |
| **R3.a** (`vil_memory` retirement) | Blueprint §6 Risk 2 requires `VacMemoryBridge` inserted in `vil_swarm` first; multi-day and cascades into the planner/executor agents. |
| **R3.b** (MCP primary swap) | Legacy `McpConnectionState` has callers across TUI + CLI tests. Partial bridge landed in this cycle; full cutover is a separate sed+audit cycle. |
| **R3.c** (ACP primary swap) | `vac_core::AcpServer` in `commands::acp.rs` still authoritative. `vac_bridge::AcpServer` exercised via its own integration tests only. Requires the approval-mediator migration. |
| **R5.a** (Candle real backend) | Paket E / B1 — decision pending on `candle-core` vs `wgpu` GGUF loader. 3–4 days. |
| **R5.b** (HNSW persistence) | `LinearAnnIndex` ships the surface (this cycle). Swap to `instant-distance::HnswMap` + `.vac/rag/index.bin` persistence is 2 days — blocked on a concrete corpus-size trigger. |
| **R5.c** (rust-analysis via portable-pty) | `ra_ap_*` API churn on unstable 0.0.x. Feature-gated stub remains; real spawn + `initialize` + diagnostic parse is 3–4 days. |
| **R6.c** (`vac_ux_services` extract) | Seven UI-agnostic services live inside `vac_tui_runtime`. Moving to a leaf crate requires updating downstream re-exports; mechanical but large-surface. |
| **R6.d** (`check_layering.sh` L1–L5) | Current gate guards `vil_* → vac_*` only. Extending to the 5-layer architecture diagram needs per-crate `layer = "Lx"` metadata and a sort pass in the script. |

### Net landed this cycle

Commits: **2445c0a, e36ab83, 1f0d897, 4e70d19, d7c1d2e + current**.
Tests: ~40 new (adapter translation, engine mode resolver, bulk
approval helpers, ranked BM25 search, service reachability, MCP core
bridge, linear search path). Every commit runs the workspace `cargo
check --tests` gate clean.

Blueprint §5 "must" cut line (Rings 0, 1, 2) is **landed in every
way that doesn't require a two-week coexistence window**; "should"
(Ring 3) and "nice" (Rings 4, 5, 6) each have at least one concrete
landing plus a tracked follow-up.
