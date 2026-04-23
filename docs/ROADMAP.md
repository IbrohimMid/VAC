# VAC Roadmap — Canonical Reading Order

**Updated:** 2026-04-23

This file is the single pointer into VAC's planning + product docs.
Planning history accreted as the project ran — rather than rewrite
every prior doc, this page pins **which doc is authoritative for
which question** and flags superseded material.

---

## Start here (in order)

1. **[PRODUCT_SPEC.md](PRODUCT_SPEC.md)** — what VAC is, value props,
   feature surface. Read first for the "why".
2. **[architecture.md](architecture.md)** — how the 31 crates fit
   together. Layered view, invariants, extension points.
3. **[ultraplan-vac-product.md](ultraplan-vac-product.md)** ✦
   **authoritative plan** ✦ — evidence-anchored milestones + three
   product threads (proactive assistant, remote deep planner, swarm
   team). Trumps earlier blueprints.
4. **[adoption-score.md](adoption-score.md)** — current score
   (66.7% weighted) with evidence per G1..G14. Updated per wave.
5. **[deployment.md](deployment.md)** — how to install + run.

## Reference docs (read as needed)

- **[completion-blueprint.md](completion-blueprint.md)** — original
  7-ring convergence plan. **Superseded by ultraplan §3 milestone
  table**; the §8 ledger is still a useful historical record of
  what landed in the R0..R6 autonomous run.
- **[hundred-percent-blueprint.md](hundred-percent-blueprint.md)** —
  closure plan for G1..G14. **Absorbed into ultraplan §3 + §4**;
  M1..M14 descriptions are duplicated there with evidence anchors.
- **[implementation-plan.md](implementation-plan.md)** — original
  10-Fase plan that produced the current codebase. **Historical
  record**; all Fases F0..F10 have shipped.

## Operational docs

- **[RUNBOOK.md](RUNBOOK.md)** — incident response + recovery.
- **[RELEASING.md](RELEASING.md)** — tag + release flow.
- **[SECURITY.md](SECURITY.md)** — reporting + model.
- **[THREAT_MODEL.md](THREAT_MODEL.md)** — attack surface.
- **[privacy_architecture.md](privacy_architecture.md)** — redaction
  pipeline + vault.
- **[TELEMETRY.md](TELEMETRY.md)** — OTel export, signed exports.
- **[PROVIDER_PARITY.md](PROVIDER_PARITY.md)** — LLM provider matrix.
- **[STRUCTURED_APPROVAL_FLOW.md](STRUCTURED_APPROVAL_FLOW.md)** —
  approval state machine.
- **[runtime_operating_guide.md](runtime_operating_guide.md)** —
  autopilot + runtime operator runbook.
- **[RUNTIME_QUEUE_BOUNDARY.md](RUNTIME_QUEUE_BOUNDARY.md)** —
  queue semantics.
- **[ux-contract.md](ux-contract.md)** +
  **[ux-self-audit.md](ux-self-audit.md)** — UI invariants.
- **[boot-timing.md](boot-timing.md)** — boot profile reference.

## Historical + analysis

- **[COMPETITIVE_ANALYSIS.md](COMPETITIVE_ANALYSIS.md)** —
  comparison vs Stakpak, Trae, Claude Code, OMNI. Feeds ultraplan.
- **[adoption-status.md](adoption-status.md)** — older adoption
  audit; superseded by `adoption-score.md`.
- **[ablation-guide.md](ablation-guide.md)** — feature-ablation
  playbook.
- **[onboarding.md](onboarding.md)** — contributor onramp.
- **[internal_deployments.md](internal_deployments.md)** —
  VAC-Vastar-internal notes.
- **[STABILITY_LOG.md](STABILITY_LOG.md)** — stability journal.

---

## Doc status cheat sheet

| Status | Meaning |
|---|---|
| ✦ **authoritative** | This is the single source of truth. |
| 🟢 active | Current; update as plan executes. |
| 🟡 reference | Still useful but not the driver. |
| 🔵 historical | Snapshot of a past moment; don't edit in place. |
| ⚫ superseded | Replaced by a newer doc; marked in its own header. |

| Doc | Status |
|---|---|
| `ultraplan-vac-product.md` | ✦ authoritative plan |
| `adoption-score.md` | 🟢 active (regenerate per wave) |
| `PRODUCT_SPEC.md` | 🟢 active |
| `architecture.md` | 🟢 active |
| `deployment.md` | 🟢 active |
| `completion-blueprint.md` | ⚫ superseded by ultraplan §3 |
| `hundred-percent-blueprint.md` | ⚫ superseded by ultraplan §3/§4 |
| `implementation-plan.md` | 🔵 historical (F0..F10 shipped) |
| `COMPETITIVE_ANALYSIS.md` | 🟡 reference (input to ultraplan) |
| `adoption-status.md` | ⚫ superseded by `adoption-score.md` |

---

## How the plans relate

```
                 COMPETITIVE_ANALYSIS.md
                        │
                        ▼
              implementation-plan.md (F0..F10)
                        │   [shipped]
                        ▼
              completion-blueprint.md (R0..R6)
                        │   [superseded]
                        ▼
         hundred-percent-blueprint.md (M1..M14)
                        │   [superseded]
                        ▼
        ultraplan-vac-product.md ← ✦ current plan
                        │
                        ▼
         adoption-score.md ← ← ← regenerated per wave
```

The four planning docs aren't redundant — each represents the plan
at a specific decision point. `ultraplan-vac-product.md` is the one
to execute from; the others document how we got here.

---

## Execution order (from ultraplan §6)

**Wave 1 — foundation (10 days).** M1 boot phases, M8 app shell
trim, M10 ingest cache, M2 engine convergence + M2.1 budget + M2.2
file-history-by-submit + M9 resume e2e. End-state score: **79.5%**.

**Wave 2 — contracts + subsystem convergence (12 days).** M3 + M3.1
+ M3.2 tool contract, M5 + M5.1 MCP primary swap, M4 TrustGate
unified, M6 + M6.1 bridge e2e, M7.1/.2/.3 memory four-phase +
triggers + vil_memory retire. End-state score: **93%**.

**Wave 3 — backends + product threads (15 days).** M11 Candle, M12
rust-analyzer, M13 autopilot cron fires, P1 proactive assistant,
P2 remote deep planner, P3 swarm team + speculation. End-state:
**100%** + three product threads live.

---

## Commit discipline

Milestone commits: `<area>(M<N>.<sub>): <short>`.
Wave reconciliation: `docs(wave<N>): regenerate adoption-score`.
History queryable: `git log --grep='M2\|M7' --oneline`.
