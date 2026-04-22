# Ablation Guide

How to produce research-grade comparisons between agent policies /
decision sequences using VAC's existing trajectory + strategy
primitives. Intended for reviewers who need to reproduce a claim about
agent behavior, not for day-to-day operators.

## Building blocks

| Piece | Source | What it provides |
|---|---|---|
| `AgentStrategy` trait | `vil_swarm::strategy` | policy boundary with two ref impls (default, conservative) |
| `SwarmConfig.strategy` | `vac_core::config` | config-driven strategy selection |
| `TraceRecorder.record_agent_decision` | `vac_trace::recorder` | persist chosen / rejected / rationale per turn |
| `vac_trajectory::decisions` | crate | extract + score decisions |
| `vac eval --golden` | CLI | per-pair comparison + match-rate % |
| `vac eval --minimal` | CLI | disable trace / memory / mcp for clean runs |
| `VacConfig::minimal()` | `vac_core::config` | side-effect free config constructor |

## Recipe — ablating strategy on a fixed prompt

1. **Record a baseline.** Run the agent once under default strategy on
   your task. This produces `.vac/traces/<session>.json` with
   `AgentDecision` records (emitted per runtime `ToolCall`).

2. **Extract decisions as golden.**
   ```bash
   vac --format json decisions .vac/traces/<session>.json > golden.json
   jq '.records' golden.json > golden-decisions.json
   ```

3. **Flip the strategy via config.** Add to `.vac/config.toml`:
   ```toml
   [swarm]
   strategy = "conservative"
   ```

4. **Run a second trace**, same prompt.

5. **Compare.**
   ```bash
   vac --format json eval .vac/traces/<second-session>.json \
       --golden golden-decisions.json \
       --minimal \
       --succeeded true \
       --duration-ms 45000 > ablation-report.json
   ```

6. **Read the match rate.** `golden_match_rate_pct` and
   `golden_details` in the JSON capture how often the conservative
   strategy picked the same tool as the baseline.

## Recipe — scoring a custom decision heuristic

`DecisionScore` lives in `vac_trajectory::decisions::score_decisions`.
The default heuristic is intentionally simple (success +30, rationale
bonus, alternatives bonus, duration penalty) so downstream tools can
override it.

For a custom score, build on the raw extraction:

```rust
use vac_trajectory::decisions::{load_decisions_from_file, DecisionRecord};

let records: Vec<DecisionRecord> =
    load_decisions_from_file(Path::new("trace.json")).await?;
let score = my_research_score(&records);
```

## Primitives reference

- `strategy_from_name(s) -> Box<dyn AgentStrategy>` — dynamic dispatch
  by name.
- `SwarmOrchestrator::strategy_choose(ctx) -> StrategyAction` — ask
  the configured strategy what to do, without committing to it.
- `SwarmOrchestrator::strategy_name()` — observability.

## What this does not provide

- No sandbox isolation between ablation runs — you're responsible for
  clean `.vac/` state between experiments (`rm -rf .vac/traces/*`).
- No statistical framework. The match-rate % is a single-sample
  measure; aggregate across runs yourself.
- No automatic prompt variation — ablation operates on fixed prompts.

## See also

- `docs/adoption-status.md` — current ablation completeness per donor.
- `crates/vil_swarm/src/strategy.rs` — trait source + tests.
- `crates/vac_trajectory/src/decisions.rs` — scoring source.
