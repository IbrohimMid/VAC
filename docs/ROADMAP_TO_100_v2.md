# Roadmap to 100/100 — v2 (Codebase-Grounded)

**Supersedes `ROADMAP_TO_100.md` (v1).** v1 scored 82/100 in external review. This v2 applies five corrections: rebaseline, un-mark false-done items, reframe provider work as *runtime-truth unification*, split operability vs evidence-gate, and move desktop/web expansion out of critical path.

Change labels used throughout: **KEEP** (unchanged from v1) · **REWRITE** (semantics changed) · **REMOVE** (dropped from this track) · **MOVE** (relocated to another track) · **NEW** (added in v2).

---

## Baseline decision (NEW)

Roadmap ini punya dua baseline eksplisit:

| Scope | Baseline |
|---|---:|
| **Historical** (post-PR #15, pre-patch) | 70/100 |
| **Current main** (post-today's quick-fix patch) | **74/100** |

Scoreboard eksekusi pakai **current-main baseline = 74**. Skor delta per fase dihitung dari 74, bukan dari 70. Angka 70 hanya dipakai untuk narasi historis "berapa yang sudah kita kunci sebelum fase berikut dimulai".

Alasan 74 (bukan 76+): kenaikan pasca PR #15 dari substansi feature (ContextCrawler real-wired, FSM in checkpoint, VIL workbench, ask-user structured, banner severity, trace events kaya) masih di-offset oleh security primitive false-secure yang baru saja lahir. Quick-fix hari ini menutup ~1–2 poin dari utang itu (policy gate fail-closed + lock-across-await + silent-error logging), tapi tiga primitive inti (secret detector, bundle import, classifier) masih utang penuh.

---

## Sumbu skor

Tidak berubah dari v1.

| Dimensi | Bobot | Baseline | Target |
|---|---:|---:|---:|
| Core architecture & control plane | 15 | 9.0 | 15 |
| Safety / trust / audit | 15 | 7.0 | 15 |
| Operator UX / TUI | 12 | 8.5 | 12 |
| Extensibility (MCP / Rulebook / LSP) | 10 | 7.7 | 10 |
| Testing / CI maturity | 12 | 7.2 | 12 |
| Release engineering / distribution | 18 | 3.0 | 18 |
| Production operability | 18 | 6.7 | 18 |
| **Total** | **100** | **~74** | **100** |

---

## Fase 0 — Stabilisasi (1–2 minggu) — *block-prevention, +0 poin*

**Audit ulang dari v1.** Label baru per item:

### Genuinely landed in main (verified today)
- [x] **KEEP** Restore `.cargo/config.toml` (shared target-dir aktif lagi).
- [x] **KEEP** `tokio::spawn` → `spawn_blocking` utk bundle FS (`runner.rs`).
- [x] **KEEP** Lock-across-await fix di `agent_scheduler::set_worker_status`.
- [x] **KEEP** `try_lock().unwrap()` → `blocking_lock()` di `vil_llm::router`.
- [x] **KEEP** `warn!` log di approval/queue silent failures.
- [x] **KEEP** Policy gate Strict + score=None → **Block** (fail-closed).
- [x] **NEW (just patched)** `PolicyGateMode::parse` — unknown value sekarang return `None`; caller `evaluate` default ke `Strict` (fail-closed) dengan `warn!` log. Sebelumnya v1 menandai ini selesai padahal belum — reviewer benar.

### Still pending in main
- [ ] **KEEP** Hapus 2x duplikat event handler di `engine.rs` (~lines 619-680 & 997-1058).
- [ ] **REWRITE** FSM `SetRetry` bug: orchestrator saat ini emit `SetRetry` bahkan pada iterasi sukses terakhir → status reasoning melapor "Retry" untuk run sukses. Fix: emit `SetRetry` hanya kalau iterasi berikutnya akan benar-benar berjalan. (v1 sudah sebut, tapi di bawah Fase 2; dipindah ke Fase 0 karena merusak kebenaran trace — block untuk audit Fase 1.)
- [ ] **KEEP** `context_crawler.rs:194` byte-slice panic pada path multibyte (porcelain rename parser).
- [ ] **NEW** Clippy gate: `cargo clippy --workspace --all-targets -- -D warnings` di CI (saat ini clippy tidak gate merge).

**Exit:** `cargo test --workspace` hijau, clippy gate hijau, no behavioral regression vs pre-PR #15.

---

## Fase 1 — Security Primitive Hardening (3–4 minggu) — *+8 poin*

**KEEP** seluruh sub-tree dari v1. Reviewer juga menilai ini fase paling kuat.

Satu tambahan eksplisit:

- [ ] **NEW** Threat model dokumen di `docs/THREAT_MODEL.md`: assets (session state, approval records, trace), trust boundaries (import, MCP server, user shell), adversary model (malicious bundle author, compromised npm upstream, malicious MCP server). Dipakai sebagai input tes adversarial di 1.1–1.3.

Sub-fase 1.1 (secret detector), 1.2 (bundle import), 1.3 (policy classifier) tidak berubah.

**Skor delta: +8** (v1 klaim +8.5 tapi sedikit overlap dengan item Fase 0 yang baru saja di-patch).

---

## Fase 2 — Architectural Debt Cleanup (2–3 minggu) — *+2 poin*

### 2.1 Reasoning FSM legal-transitions — **MOVE** bagian SetRetry-fix ke Fase 0
- [ ] **KEEP** Tambah `legal_transitions` table + `apply` reject illegal.
- [ ] **KEEP** Test: FSM menolak Observe sebelum BeginAttempt, dsb.

### 2.2 Scheduler/Autopilot konsolidasi — **REWRITE** (arah belum pasti)
Reviewer benar: dua substrate tidak otomatis harus jadi satu. Ubah jadi **audit task** dulu.
- [ ] **NEW** Audit: dokumentasikan domain boundary antara `TaskQueue` (generic jobs) dan `AgentTaskQueue` (role-based agent work). Justify retention atau merge.
- [ ] **REWRITE** Jika retention: buat adapter layer `RuntimeQueue` trait + test; TUI baca dari satu abstraction. Jika merge: migration runner + back-compat test.

### 2.3 Event handler dedup — **KEEP**
### 2.4 Stringly-typed config — **KEEP** (dan tambah:)
- [ ] **NEW** `LlmConfig` source-of-truth: hari ini ada dua — `vac_core::config::LlmConfig` dan `vil_llm::config::LlmConfig`. Pilih satu (rekomendasi: `vil_llm` sebagai owner, `vac_core` hanya re-export). Rename atau delete yang lain. Ini prerequisite untuk Fase 3.

**Skor delta: +2** (v1 bilang +2.5; turun karena 2.2 sekarang mungkin "audit-only" tanpa consolidation).

---

## Fase 3 — Runtime-Truth Unification (3–4 minggu) — *+3 poin*

**REWRITE TOTAL** dari v1 ("Provider Parity"). Reviewer benar: framing v1 salah. Providers *sudah ada* di `vil_llm::router` (Anthropic, Gemini, OpenAI, OpenAiCompat, mistral, xai). Masalahnya adalah:

1. `LlmRouter::from_config()` hanya register `anthropic`/`kilo`/`kilo_gateway`; sisanya di-skip dengan warning.
2. `vac_core::engine` masih import `AnthropicProvider` langsung & build provider manual via `build_kilo_provider()` — runtime bypass config surface.
3. Dua `LlmConfig` struct berbeda (ditangani di 2.4).

### Tasks
- [ ] **NEW** `LlmRouter::from_config` register **semua** provider yang punya kredensial valid di config (tidak skip).
- [ ] **NEW** `vac_core::engine` hapus `use vil_llm::AnthropicProvider` dan `build_kilo_provider()`; ganti dgn `LlmRouter::from_config(cfg)` sebagai satu-satunya construction path.
- [ ] **KEEP** Smoke matrix CI: env-gated, minimal "say hi" per provider yang punya secret di CI.
- [ ] **KEEP** Streaming parity test (tool-call streaming) per provider; dokumentasikan gap eksplisit jika ada.
- [ ] **NEW** Integration test: swap provider via config file tanpa recompile, verifikasi engine pick up.

**Exit:** engine compile tanpa `use …Provider` concrete import; smoke matrix hijau untuk ≥3 provider; config round-trip identik.

**Skor delta: +3** (v1 +2.5; naik karena scope sekarang lebih ambisius — engine refactor + config unification, bukan sekadar wire providers).

---

## Fase 4 — Testing / CI Maturity (2–3 minggu) — *+4.8 poin*

**KEEP** seluruhnya dari v1. Reviewer setuju.

Satu penambahan:
- [ ] **NEW** `cargo-mutants` (mutation testing) untuk `vac_core::security::*` dan `vac_core::policy_gate` — security code wajib lulus mutation score ≥80%.

---

## Fase 5 — Release Engineering & Distribution (4–6 minggu) — *+15 poin*

**KEEP** seluruhnya dari v1. Reviewer menilai ini fase paling penting dan bobot sudah tepat.

---

## Fase 6A — Operability Engineering (2–3 minggu) — *+6 poin*

**REWRITE** dari v1 Fase 6. Pisahkan engineering work dari evidence gate.

### Tasks
- [ ] **KEEP** Structured logs (JSON) + `RUST_LOG` per-module.
- [ ] **KEEP** OpenTelemetry traces opsional (`--otel-endpoint`).
- [ ] **KEEP** Metrics endpoint (Prometheus format) utk autopilot/scheduler.
- [ ] **REMOVE** `vac doctor --json` — reviewer catat: sudah ada di codebase.
- [ ] **KEEP** Crash dump capture: panic hook → `~/.vac/crashes/<ts>.json` (redacted).
- [ ] **KEEP** Memory/disk quota enforcement.
- [ ] **KEEP** Tool execution timeout audit (tiap tool punya timeout).
- [ ] **NEW** Trace trust boundary audit: verify export/import/trace semua melewati satu redaction contract (sambung ke Fase 1.1 detector upgrade). Trace recorder saat ini menulis `content` + full `tool_calls` JSON unredacted — ini menjadi leak vector kalau trace file di-share.

**Exit:** observability stack live di staging/internal deploy; crash hook teruji; trace redaction pass corpus test.

**Skor delta: +6.**

---

## Fase 6B — Production Evidence Gate (4–8 minggu wall-clock, bukan engineering effort) — *+5.3 poin*

**NEW** (dipisah dari v1 Fase 6). Ini **bukan** fase engineering; ini periode observasi pasca-1.0.

### Gates
- [ ] Release stability: 4 minggu tanpa critical bug setelah 1.0 tag.
- [ ] ≥3 internal case study deployment terdokumentasi.
- [ ] Public changelog: zero retracted release dalam window.
- [ ] Public security disclosure policy (`SECURITY.md`) dipublikasikan.
- [ ] 3rd-party re-audit (re-engage reviewer + VIL kode audit) — delta vs baseline v2 terverifikasi.

Fase ini **paralel** dengan Fase 5 tail & Fase 6A tail; tidak mem-block engineering.

**Skor delta: +5.3.**

---

## Fase 7 — ~~Benchmark Parity Polish~~ **REMOVED dari critical path**

**MOVE** ke `docs/ROADMAP_V1X.md` (post-1.0 expansion track). Items:

- Desktop app shell (Tauri) — **MOVE**
- Plugin marketplace / rulebook registry — **MOVE**
- Web app surface — **MOVE**
- LSP parity multi-language — **MOVE**

Justifikasi: benchmark utama yang user cita-citakan (`stakpak/agent`) mencapai product-grade **tanpa** desktop/web/marketplace. OpenCode punya itu, tapi itu expansion play, bukan prerequisite production-grade. Kalau dipertahankan di critical path, skor stretch tapi focus melebar.

**Skor delta: 0 (tidak dihitung menuju 100).**

---

## New sum timeline

| Fase | Durasi | Δ skor | Kumulatif |
|---|---:|---:|---:|
| 0 — Stabilisasi (sisa) | 1–2 wk | 0 | 74 |
| 1 — Security primitives | 3–4 wk | +8 | 82 |
| 2 — Arch debt cleanup | 2–3 wk | +2 | 84 |
| 3 — Runtime-truth unification | 3–4 wk | +3 | 87 |
| 4 — Testing/CI maturity | 2–3 wk | +4.8 | 91.8 |
| 5 — Release engineering | 4–6 wk | +15 → capped +8 | **99.8** (capped by ops) |
| 6A — Operability engineering | 2–3 wk | +6 | — |
| 6B — Production evidence gate | 4–8 wk wall-clock | +5.3 | — |

**Total engineering: 17–25 minggu** (turun dari v1 19–29 karena Fase 7 dikeluarkan).

**100/100 tercapai setelah 6B lulus**; skor di engineering-completion (akhir 6A) sudah ≥97, sisa 3 poin hanya bisa diklaim via evidence gate, bukan via kode.

Catatan "capped": delta Fase 5 dan 6A overlap pada produksi-ready-ness; total kontribusinya tidak aditif naif. Scoreboard final dicek per dimensi, bukan sum sederhana.

---

## Installer / first-run smoke contract (NEW)

Reviewer benar — per-channel smoke cukup minimal di v1. Tingkatkan ke:

```
install via channel X
  → vac --version           (exit 0, semver parse)
  → vac doctor              (exit 0, no ERROR lines)
  → vac init                (creates .vac/, idempotent)
  → vac runtime status      (no-op but valid output)
  → vac task "say hi"       (smoke LLM path; env-gated; or mock)
  → config roundtrip:
      vac config show > cfg1
      vac config set foo=bar && vac config show > cfg2
      diff expected vs cfg2
```

Smoke ini jadi exit criterion Fase 5 per channel.

---

## Label audit summary (v2 vs v1)

| Item | Label |
|---|---|
| Fase 0 item list | KEEP sebagian, REWRITE (add SetRetry fix), NEW (clippy gate, PolicyGateMode correction) |
| Fase 1 seluruh sub-tree | KEEP + NEW threat model |
| Fase 2.2 scheduler consolidation | REWRITE (audit-first) |
| Fase 2.4 add LlmConfig owner | NEW |
| Fase 3 "provider parity" framing | REWRITE → runtime-truth unification |
| Fase 4 | KEEP + NEW mutation testing |
| Fase 5 | KEEP seluruhnya |
| Fase 6 | REWRITE → split 6A engineering / 6B evidence gate |
| Fase 6 `vac doctor --json` | REMOVE (sudah ada) |
| Fase 6 trace redaction | NEW |
| Fase 7 desktop/web/plugin | MOVE ke `ROADMAP_V1X.md` |

---

## Anti-pattern (KEEP dari v1, tanpa perubahan)

1. Klaim tanpa verifikasi.
2. Feature-flag perpetual.
3. Skip security untuk speed.
4. Cloud-agent batch tanpa human review disiplin.
5. Stringly-typed contracts.

---

## Verdict gate (tightened)

Skor 100/100 hanya boleh diklaim kalau **semua** berikut benar secara verifikasi:

- Semua exit criteria Fase 0–6A ditandai ✅ dengan link ke test/CI/artefak.
- Fase 6B: 4 minggu stability streak + ≥4 signed channels + 3rd-party re-audit delta positif.
- Skor per-dimensi (bukan sum): tidak ada dimensi <target.
- Threat model dipublikasikan + security disclosure policy aktif.
- Public changelog + `SECURITY.md` + `RELEASING.md` + `docs/RUNBOOK.md` lengkap.

Sebelum itu: framing publik tetap *"strong beta / near-production"*.
