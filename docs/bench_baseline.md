# Compete-Blueprint Baseline — 2026-04-24

Snapshot captured at T0.1 before the four-tentpole arc begins.
Later phase gates compare against these numbers.

## Repo snapshot

- Branch: `main`
- HEAD: `fb58f03 docs: agent-executable plan for the compete-blueprint arc`
- Crate count: 25 (workspace members in `Cargo.toml`)

## UX gap analysis tally

| Severity | Count |
|---|---|
| 🟢 fully surfaced | 35 (grep occurrences, some multi-column) |
| 🟡 partial | 7 |
| 🔴 silent | 16 (grep occurrences; ~2 distinct items per ROADMAP) |

Note: the 🔴 grep count is inflated by historical table rows
that describe past silent states; the canonical "still silent"
list lives in ROADMAP §Deferred and names exactly 2 items.

## Pulse facets (post-arc: 13)

- `SystemPulse::facets().len()` = **13**
  (approvals / runtime / mcp / shell / speculation / environment /
  budget / memory / policy / lsp / tasks / cron / subagent)
- Contract tests: 13 facets, 12 separators, ≤120-char compact
  line (scaled up from 100 at C.2).

## Bridge allowlist (post-arc: 19)

- `BRIDGE_ALLOWLIST` entries = **19**
  (trust / env / mcp / spill / policy / rate / memory / resume /
  spec / lsp / compact / gate / skills / cron / schedule /
  monitor / hooks / web / rewind)

## Session-engine tests

| File | Tests |
|---|---|
| `submit.rs` (inline) | 4 |
| `tests/contracts.rs` | 9 |
| `tests/provider_matrix.rs` | 9 |
| `tests/slas.rs` | 3 |

Total engine tests = 25.

## Benchmarks

A.6 landed the `bench_submit_stream_first_chunk` criterion
bench + a wall-clock SLA test. Post-arc numbers (EchoAdapter,
tempfs, M1-class host):

- `submit_stream_first_chunk_under_250ms` — passes consistently
  at ~20–60 ms first non-Accepted chunk. Ceiling left loose
  (250 ms) so CI on slower runners stays green; the criterion
  bench is the drift-tracking sample.
- 169 / 169 engine tests green post-arc (up from 25 at T0.1).

Build wall-time slots remain unrecorded — cold-rebuild cost is
dominated by the 583-dep tree, not arc additions.

## Invariants Phase A must preserve

1. Transcript: `Accepted → {Slash | LlmRequest → LlmResponse} →
   {Finished | Aborted}` row ordering, one Accepted per submit.
2. Durability: Accepted row fsynced before first adapter call
   (enforced by `submit_one_writes_accepted_before_llm_response`).
3. Pulse: facet count + separator count match the contract tests.
4. Bridge: BRIDGE_ALLOWLIST is append-only during the arc —
   no label renames.

If any of these moves mid-phase, it's a bug, not a feature.
