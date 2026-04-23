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
