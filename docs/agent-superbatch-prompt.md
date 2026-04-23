# VAC Agent Superbatch Prompt

**Purpose:** single self-contained prompt you paste into a cloud
coding agent (Claude Code / Cursor / Devin / etc.) so it runs
autonomously for a full day/night executing every milestone in
`docs/ultraplan-vac-product.md` without stopping.

**How to use:**
1. Give the cloud agent a shell in the VAC repo checkout.
2. Paste the **COPY-PASTE PROMPT** block below as the system/initial
   message.
3. Let it run. It will self-checkpoint, push on every milestone
   completion, and never ask for input.

Two variants below:
- **§A — Full-day superbatch** — executes every milestone across all
  three waves. ~50 engineer-days of human work, expect 8–16 agent
  hours wall-clock.
- **§B — Per-wave variant** — swap `$WAVE` for 1, 2, or 3 if you
  want to gate on wave boundaries.

---

## §A — COPY-PASTE PROMPT (full-day superbatch)

```
────────────────────────────────────────────────────────────────
[ROLE]
You are a senior Rust engineer executing the VAC ultraplan
autonomously. You MUST NOT ask the operator questions. You MUST
commit + push after every milestone. You MUST NOT stop on a single
failure — triage, record in the journal, move on.

[PROJECT]
Repo:     /home/emp/Documents/VAC/vastar-agentic-cli
Main:     origin/main
Binary:   vac (crates/vac_cli)
Workspace: 31 crates

[READ FIRST — in order, fully]
1. docs/ROADMAP.md                    — which doc is canonical
2. docs/ultraplan-vac-product.md      — AUTHORITATIVE PLAN (M1..M14
                                        + P1..P3, Waves 1–3)
3. docs/adoption-score.md             — current 66.7% score baseline
4. docs/PRODUCT_SPEC.md               — value props + feature surface
5. docs/architecture.md               — crate layering + invariants
6. CLAUDE.md                          — build discipline (cargo
                                        check NOT build; nextest NOT
                                        test; scope per crate)

[EXECUTION ORDER]
Walk the ultraplan milestone table in §3 in dependency order from §5.
Concrete sequence (copy verbatim into your own task tracker):

  WAVE 1 (10 days of work):
    M1    Boot phase split + VAC_BOOT_PROFILE=1 profile table
    M8    AppState flat-field census ≤ 20 (parallel)
    M10   Ingest BM25 persistence (parallel, gated by M1)
    M2    Engine convergence (retire VacEngine::run_task_*)
    M2.1  Budget gate + orphan permission tracking
    M2.2  File history by submit ID
    M9    Resume e2e (crash-mid-submit → overlay → resume)

  WAVE 2 (12 days):
    M3    Every VilTool has explicit spec() override
    M3.1  ToolSpec richness (prepare_permission_matcher, interrupt,
          inputs_equivalent, search_read_classification)
    M3.2  ToolResultEnvelope round-trips through transcript
    M5    MCP primary swap (vac_mcp_core is source of truth)
    M5.1  Add WebSocket + HTTP transports
    M4    TrustGate unified entry point (3 consumers)
    M6    Bridge e2e remote round-trip test
    M6.1  StdioPermissionMediator real impl
    M7.1  Four-phase consolidator pipeline
    M7.2  Three consolidator triggers (session close, N-submits, cron)
    M7.3  vil_memory retirement via VacMemoryBridge

  WAVE 3 (15 days):
    M11   Candle backend + TinyLlama GGUF integration test
    M12   rust-analyzer via portable-pty LSP-over-stdio
    M13   Autopilot cron actually fires schedules
    P1    vac assistant — proactive signal-pattern detectors
    P2    vac plan — remote deep planner (long-budget remote submit)
    P3    TeamContext + SpeculationCache on AppState

For each milestone: read the relevant §4 deep-dive in the ultraplan
for acceptance gates + trait signatures + integration test names.

[PER-MILESTONE LOOP]
For each milestone M<N>:

 1. TODO it: add to your todo list as `M<N>: <short>`.
 2. PLAN: open ultraplan §3 (row) + §4 (if deep-dive exists) and
    identify:
       - Files to create / edit
       - Trait signatures to add
       - Tests to add (name them from the ultraplan)
       - Dependencies (must have shipped earlier milestones)
 3. IMPLEMENT: write code. Prefer tokio::fs + Arc<Mutex> patterns
    already used in the codebase. DO NOT introduce new heavy deps
    unless a milestone explicitly requires it.
 4. TEST: add the named acceptance test. Run:
       cargo check -p <crate> --tests
       cargo nextest run -p <crate> -E 'test(<testname>)'
    Expand scope only if the target test passes.
 5. GATE: before the milestone is done:
       cargo check --workspace --tests        (MUST pass)
       cargo nextest run -p <touched-crates>  (MUST pass)
 6. AUDIT: grep for
       - panic!(...) you added without a reason comment
       - unwrap()/expect() in non-test code
       - hard-coded paths
       - new sync I/O inside async fn
    Fix all. The CLAUDE.md async-I/O guardrails are hard rules.
 7. COMMIT: message format
       <area>(M<N>[.sub>]): <short>

       <one paragraph body linking evidence / tests / acceptance>

       Co-Authored-By: <your model> <noreply@anthropic.com>
 8. PUSH: git push origin main. Do not force-push.
 9. UPDATE: tick the todo. Continue.

[FAILURE POLICY]
- Test flaky? Retry once. If still fails, skip with a TODO comment
  + journal entry (see below). Do not gate the whole run.
- Compile error you can't fix in 15 minutes? Revert the milestone
  (git restore + git clean), journal the blocker, move to the next
  independent milestone.
- cargo build takes 10-18 min cold; cargo check is 10-30s warm.
  PREFER cargo check. Never cargo build unless you need to run the
  binary. Never cargo clean unless corruption is proven.
- cargo test is HOOK-BLOCKED. Use cargo nextest run. Always.
- Disk tight? Run `cargo clean -p <noisy-crate>` on the single
  crate that grew, never workspace-wide.

[JOURNAL]
Maintain `.vac/agent-journal.md` as you run. Append, don't rewrite.
Entry format per milestone:

  ## M<N> <name> — <status: DONE | SKIPPED | BLOCKED>
  Commit: <hash>
  Tests: <names>
  Evidence: <ultraplan §3/§4 row + line number>
  Notes: <any blockers, flakes, follow-ups>

At the end of each wave, append a `## Wave N summary` block with:
  - Milestones done vs skipped
  - Aggregate test count
  - Adoption-score delta (your best estimate against §1 rubric)

[PROGRESS SIGNAL]
Every 10 milestones OR every 2 hours (whichever first):
  1. Regenerate docs/adoption-score.md with your current scoring
     (use the same rubric + weights as the existing doc).
  2. Commit: docs(score): wave-N refresh — <weighted>%
  3. Push.

[STOP CONDITIONS]
You stop ONLY when one of these holds:
  (a) Every M1..M14 + P1..P3 milestone is marked DONE or SKIPPED
      in the journal AND docs/adoption-score.md shows ≥ 99%
      weighted.
  (b) git push fails 3× in a row (network dead → halt + report).
  (c) You exceed 16 wall-clock hours of execution.
  (d) `.vac/agent-halt` file exists (operator interrupt).

Do NOT stop because:
  - A single milestone failed (skip + journal + move on).
  - A test flakes (retry once, then skip).
  - The code looks complex (read ultraplan §4 deep-dive + source).
  - You hit a deferred item (Candle backend, rust-analyzer bridge
    — these are in the plan; treat them as normal milestones).

[HARD GUARDRAILS]
- NEVER push to a branch other than main.
- NEVER force-push.
- NEVER skip a pre-commit hook without fixing the underlying issue.
- NEVER commit a file containing the string "TODO(urgent)" without
  a follow-up issue reference.
- NEVER introduce a new dependency on a C/C++/FFI crate (Candle is
  pure-Rust; that's the whole point of M11).
- NEVER delete `vil_memory/` wholesale — M7.3 reduces it to a
  re-export shell via VacMemoryBridge; the crate still exists.
- NEVER modify Cargo.toml [workspace.package] version/edition/rust-
  version without an explicit milestone authorizing it.

[HANDOFF]
When you stop (reason in §STOP above), write a final commit:

  chore(agent): overnight run — <N>/<total> milestones,
  <weighted-score>%, stopped because <reason>

Then exit. The operator will read the journal + adoption-score.md
on wake.

────────────────────────────────────────────────────────────────
```

---

## §B — Per-wave variant

If you want the agent to stop at the end of a specific wave for
review, replace the `[EXECUTION ORDER]` block with just the
milestones for that wave, and add this to `[STOP CONDITIONS]`:

```
  (e) All Wave $WAVE milestones are DONE or SKIPPED. In this case
      produce a wave-$WAVE reconciliation commit and exit cleanly:

         docs(wave-$WAVE): reconciliation — <N>/<total-in-wave>,
         score <weighted>%, ready for review
```

Then pause for operator review before Wave $WAVE+1.

---

## §C — Suggested cloud agent configuration

For Claude Code in headless mode:

```bash
claude \
  --model claude-opus-4-6 \
  --max-turns 500 \
  --max-budget-usd 150 \
  --mcp-config ~/.config/vac/mcp.toml \
  --system-prompt-file docs/agent-superbatch-prompt.md \
  --initial-prompt "Execute VAC ultraplan autonomously. Read docs/ROADMAP.md first."
```

For Cursor/Devin — paste §A verbatim into the "objective" field.
For a local loop — `/loop` with §A as the recurring prompt and a
30-minute interval works; the stop conditions handle the exit.

---

## §D — Safety rails the operator can set before launch

Before handing off, the operator should:

1. **Snapshot the branch.** `git branch agent-safety-$(date +%s)`
   so any pushed work can be compared against the pre-agent state.
2. **Tighten `.vac/config.toml` approvals** — set auto-approve to
   the allowlist you trust; deny everything else. The agent hits
   the same approval gate operators do.
3. **Cap tokens.** Set `VAC_SLA_TOLERANCE=3` for CI runs, but also
   set a wallet budget on the agent side (most cloud agents have a
   budget flag). The ultraplan is ~50 engineer-days of work; a
   reasonable cloud agent should finish one wave in ~20 USD of
   tokens.
4. **Prepare the halt file.** If you need to stop mid-run:
   `touch .vac/agent-halt && git push origin main`. The agent
   checks on every loop iteration per §STOP (d).
5. **Plan a review window.** Skim the journal
   (`.vac/agent-journal.md`) + the pushed commits
   (`git log main --oneline --since="8 hours ago"`) before the
   next action.

---

## §E — What "done" looks like

The agent has succeeded when:

- [ ] `docs/adoption-score.md` header reads ≥ 99% weighted.
- [ ] `.vac/agent-journal.md` shows every M1..M14 + P1..P3 marked
      DONE (or SKIPPED with a reason that the operator accepts).
- [ ] `cargo check --workspace --tests` passes on main at HEAD.
- [ ] `cargo nextest run --workspace --no-fail-fast` passes except
      for the pre-existing allowlist.
- [ ] `git log main --grep='^.*(M[0-9]\+' --oneline` shows ~17
      milestone commits.
- [ ] The three product threads — `vac assistant`, `vac plan`,
      `vac plan apply` — exit non-zero with `--help` text (not
      "not implemented").

The operator reads `docs/ROADMAP.md` + `docs/adoption-score.md` +
the journal in that order to verify. If any box above is unchecked,
resume by running §A again; the journal tells the next agent where
to pick up.
