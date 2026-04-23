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
Commit: (to be added)
Tests: cargo check -p vac_cli --tests
Evidence: M1 in ultraplan §3
Notes: Implemented BootProfile, wrapped startup tasks into Critical and Deferred blocks.
