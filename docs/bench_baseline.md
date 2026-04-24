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

## Pulse facets

- `SystemPulse::facets().len()` = **11**
  (approvals / runtime / mcp / shell / speculation / environment /
  budget / memory / policy / lsp / subagent)
- Contract tests: 11 facets, 10 separators, ≤100-char compact line.

## Bridge allowlist

- `BRIDGE_ALLOWLIST` entries = **10**
  (trust / env / mcp / spill / policy / rate / memory / resume /
  spec / lsp)

## Session-engine tests

| File | Tests |
|---|---|
| `submit.rs` (inline) | 4 |
| `tests/contracts.rs` | 9 |
| `tests/provider_matrix.rs` | 9 |
| `tests/slas.rs` | 3 |

Total engine tests = 25.

## Benchmarks (TO CAPTURE — skipped here to keep T0.1 short)

These slots wait for Phase A to care about them; filling now
would spend 15+ minutes of cold-build wall-time for numbers that
will be superseded at A.6 anyway.

- `cargo build -p vac_cli --release` cold — unrecorded.
- `cargo build -p vac_cli --release` warm — unrecorded.
- First-token latency (EchoAdapter) — unrecorded; Phase A.6 adds
  the `first_token_under_100ms` bench that captures both sides.

## Invariants Phase A must preserve

1. Transcript: `Accepted → {Slash | LlmRequest → LlmResponse} →
   {Finished | Aborted}` row ordering, one Accepted per submit.
2. Durability: Accepted row fsynced before first adapter call
   (enforced by `submit_one_writes_accepted_before_llm_response`).
3. Pulse: facet count + separator count match the contract tests.
4. Bridge: BRIDGE_ALLOWLIST is append-only during the arc —
   no label renames.

If any of these moves mid-phase, it's a bug, not a feature.
