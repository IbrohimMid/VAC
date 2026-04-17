# Roadmap to 100/100 — Production-Grade Verdict

**Baseline (post-PR #15 + quick-fix patch, 2026-04-17): 70/100**
**Target: 100/100 — sekelas Stakpak / OpenCode / Claude Code**

Roadmap ini meng-konsolidasikan dua audit independen (Trae meta-audit 74/100 + audit kode-level VIL pasca PR #15) menjadi satu jalur eksekusi tunggal. Setiap fase punya **exit criteria yang dapat diverifikasi** (test, command, atau artefak), bukan klaim. Naik skor hanya boleh diklaim setelah exit criteria terpenuhi *dan* diverifikasi independen.

---

## Sumbu skor (7 dimensi, total 100)

| Dimensi | Bobot | Baseline | Target |
|---|---:|---:|---:|
| Core architecture & control plane | 15 | 8.8 | 15 |
| Safety / trust / audit | 15 | 6.5 | 15 |
| Operator UX / TUI | 12 | 8.5 | 12 |
| Extensibility (MCP / Rulebook / LSP) | 10 | 7.7 | 10 |
| Testing / CI maturity | 12 | 7.2 | 12 |
| Release engineering / distribution | 18 | 3.0 | 18 |
| Production operability | 18 | 6.7 | 18 |
| **Total** | **100** | **~70** | **100** |

Bobot release engineering & operability sengaja tertinggi — itu gap terbesar vs benchmark.

---

## Fase 0 — Stabilisasi (1–2 minggu) — *Block-prevention, tidak menaikkan skor*

Tidak menaikkan skor; menutup regresi yang dibawa PR #15. Sebagian sudah saya patch hari ini.

- [x] Restore `.cargo/config.toml`
- [x] Spawn-blocking utk bundle FS
- [x] Lock-across-await fix di scheduler
- [x] Strict mode fail-closed di policy_gate
- [x] `warn!` log di approval/queue silent failures
- [x] `try_lock().unwrap()` → `blocking_lock()` di router
- [ ] Hapus 2x duplikat event handler di `engine.rs` (~lines 619-680 & 997-1058)
- [ ] Fix orchestrator `SetRetry` di iterasi sukses terakhir (FSM melapor "Retry" saat run sukses)
- [ ] Fix `context_crawler.rs:194` byte-slice panic pada path multibyte (rename porcelain)

**Exit:** `cargo test --workspace` hijau, `cargo clippy --workspace -- -D warnings` hijau, no behavioral regression vs pre-PR #15.

---

## Fase 1 — Security Primitive Hardening (3–4 minggu) — *+8.5 poin*

Tiga primitive baru di PR #15 (`secret_detector`, `bundle import`, `policy_gate`) saat ini *false-secure*. Menjual kapabilitas yang tidak benar-benar ada = utang trust paling mahal. **Wajib selesai sebelum fase release engineering** karena upgrade/migration safety bergantung di sini.

### 1.1 Secret detector (Safety +3)
- [ ] Replace 7-regex implementation dgn:
  - Opsi A: port `gitleaks` rule JSON (~150 rules) ke Rust struct.
  - Opsi B: integrate crate `secrecy-detector` atau `gitleaks-rust` (audit license).
- [ ] Tambah deteksi: `ghp_*`, `ghs_*`, `gho_*`, `github_pat_*`, `xox[abp]-*`, `sk_live_*`, `AIza*`, `sk-` (OpenAI), `sk-ant-*` (Anthropic), JWT (3-segmen base64), PEM blocks (`-----BEGIN ... PRIVATE KEY-----`), SSH private keys.
- [ ] Pisahkan `SecretType::{Url, Email}` ke `PiiType` baru (bukan secret).
- [ ] Tambah Shannon entropy filter (≥4.5 bits/char) untuk generic-token rules.
- [ ] Tambah overlap dedup (interval merge).
- [ ] Adversarial test corpus: 50+ true-positives (1 per provider) + 100+ false-positive avoidance (URLs, version strings, hashes, UUIDs).

**Exit:** detector lulus corpus dgn precision ≥0.9, recall ≥0.95 di test corpus.

### 1.2 Bundle import hardening (Safety +3)
- [ ] **Signature**: tambah optional Ed25519 signature di `BundleMetadata`. Verify on import; reject jika `--require-signed` flag aktif.
- [ ] **Collision detection**: tolak import jika `session_id` sudah ada di `.vac/checkpoints/` *kecuali* user pass `--overwrite-session`.
- [ ] **Approval forgery fix**: import TIDAK boleh otomatis populate `state.approved_tools`. Persyaratkan `--trust-approvals` flag eksplisit + cetak warning.
- [ ] **Redact-on-import**: default redact mode = `On`; require `--no-redact` untuk mematikan.
- [ ] **Size cap**: `read_context_summary` cap 10 MiB; `import_bundle_from_path` total cap 100 MiB.
- [ ] **Schema version gate**: reject bundle dgn major version ≠ current.
- [ ] Adversarial roundtrip test: forged approval bundle, oversize summary, collision session_id, malformed JSON.

**Exit:** semua adversarial tests reject dengan pesan jelas; happy path tetap lulus `bundle_roundtrip`.

### 1.3 Policy gate classifier rewrite (Safety +2.5)
- [ ] Replace `classify_shell_command` dgn parser berbasis `shell-words` crate.
- [ ] Resolve argv[0] basename: handle `/usr/bin/git`, `sudo X`, `env … X`, `bash -c "X"`, `xargs X`.
- [ ] Tangani flag positional: `git -C /repo merge`, `kubectl --context=prod apply`, `terraform -chdir=foo apply`.
- [ ] Bypass corpus test: 30+ variants (sudo, absolute path, env wrapper, bash -c, flag prefix).
- [ ] Type config: `PolicyGateConfig.mode: PolicyGateMode`, `actions: Vec<PolicyGateAction>` (serde rename_all = lowercase). Reject unknown variants.

**Exit:** classifier lulus 30+ adversarial bypass tests; config schema strict.

---

## Fase 2 — Architectural Debt Cleanup (2–3 minggu) — *+2.5 poin*

### 2.1 Reasoning FSM legal-transitions (Core +0.5)
- [ ] Tambah `legal_transitions(&self) -> &[ReasoningPhase]` table.
- [ ] `apply` reject illegal transitions (return `Result<(), FsmError>`).
- [ ] Stop emit `SetRetry` di iterasi sukses terakhir (orchestrator).
- [ ] Test: success path tidak menghasilkan phase=Retry.

### 2.2 Scheduler/Autopilot konsolidasi (Core +1)
- [ ] Pilih satu queue substrate (`TaskQueue` atau `AgentTaskQueue`); convert yang lain jadi adapter.
- [ ] Satu state-file schema; TUI baca dari satu sumber.
- [ ] Migration: detect old schema, convert in-place sekali, log warning.

### 2.3 Event handler dedup (Core +0.5)
- [ ] Extract `engine.rs` 2x event-handler block jadi `handle_agent_event(rec, event)`.
- [ ] `init()` return type: `Result<Vec<String>>` → `Result<Vec<EngineWarning>>` (struct dengan code/severity/source).

### 2.4 Stringly-typed config (Extensibility +0.5)
- [ ] `PolicyGateConfig.mode/actions` enum-serde.
- [ ] `requests_per_minute: u32` → `Option<NonZeroU32>` (no magic-zero).

---

## Fase 3 — Provider Parity (3–4 minggu) — *+2.5 poin*

Audit Trae menyorot bahwa runtime masih Anthropic-centric meski config surface multi-provider. Ini blocker product-grade.

- [ ] Audit semua call site `AnthropicProvider::new` di `vac_core/engine.rs` dan `build_kilo_provider`.
- [ ] Wire `LlmRouter::from_config` sebagai entry point tunggal. Engine **tidak boleh** import provider concrete.
- [ ] Implement & test: OpenAI, Gemini, OpenRouter, Ollama (local) — minimal smoke test "say hi" per provider.
- [ ] Provider matrix CI job: env-gated, run jika kredensial tersedia.
- [ ] Streaming parity: tool-call streaming bekerja sama di semua provider, atau dokumentasikan limitation eksplisit.

**Exit:** test matrix `provider_smoke.yml` hijau di ≥3 provider; engine compile tanpa import provider concrete.

---

## Fase 4 — Testing/CI Maturity (2–3 minggu) — *+4.8 poin*

### 4.1 Coverage & adversarial
- [ ] Coverage target: 70% line untuk `vac_core`, `vac_runtime`, `vil_swarm` (use `cargo-llvm-cov`).
- [ ] Property tests (`proptest`) untuk: bundle roundtrip, FSM transition closure, secret detector idempotence, approval store invariants.
- [ ] Fuzz target untuk MCP message parser & policy classifier (`cargo-fuzz`).

### 4.2 CI hardening (matching benchmark posture)
- [ ] Replace ad-hoc CI dgn matrix:
  - `lint` (fmt + clippy -D warnings)
  - `test-linux`, `test-macos`, `test-windows`
  - `integration` (real fs, real subprocess)
  - `provider-smoke` (env-gated)
  - `release-dry-run` (cargo dist or cross)
- [ ] Cache: sccache shared across jobs.
- [ ] Required checks gate merge ke main.
- [ ] CodeQL / `cargo audit` / `cargo deny` weekly scan.

**Exit:** semua jobs hijau di main untuk 7 hari berturut-turut; coverage report di-publish.

---

## Fase 5 — Release Engineering & Distribution (4–6 minggu) — *+15 poin*

Gap terbesar (3.0 → 18). Ini yang membedakan strong-beta dari product-grade.

### 5.1 Release pipeline
- [ ] Adopt `cargo-dist` (atau `cargo-release` + custom workflow).
- [ ] Tagged releases trigger build matrix:
  - linux-x86_64-gnu, linux-x86_64-musl, linux-aarch64
  - macos-x86_64, macos-aarch64
  - windows-x86_64
- [ ] Artefak: tarball, deb, rpm, msi, sha256 checksums, SBOM (`cargo-cyclonedx`).
- [ ] Sigstore / minisign signing untuk semua artefak.
- [ ] GitHub Release notes auto-generated dari conventional commits.

### 5.2 Distribution channels
- [ ] **Homebrew tap** (`vastar/tap/vac`)
- [ ] **install script** (`curl … | sh`) — host di domain proyek
- [ ] **Docker image** (`ghcr.io/vastar/vac:tag`) multi-arch
- [ ] **Scoop bucket** (Windows)
- [ ] **AUR package** (Arch)
- [ ] (Optional) `nix flake`, `mise`, `asdf` plugin

### 5.3 Versioning & migration discipline
- [ ] SemVer commitment di `RELEASING.md`.
- [ ] `.vac/` schema versioning + migration runner (`vac migrate`).
- [ ] Backward-compat test: load `.vac/` produced by N-1 dan N-2 release.
- [ ] Deprecation policy: 1 minor cycle warning sebelum removal.

### 5.4 Installer smoke
- [ ] Per-channel post-release smoke test job: install via channel → `vac doctor` → `vac --version` → exit 0.

**Exit:** ≥4 channel aktif, signed artefak, install script lulus smoke di 3 OS, schema migration teruji.

---

## Fase 6 — Production Operability (3–4 minggu) — *+11.3 poin*

### 6.1 Observability
- [ ] Structured logs (JSON) + log levels per module via `RUST_LOG`.
- [ ] OpenTelemetry traces optional (`--otel-endpoint`).
- [ ] Metrics endpoint (Prometheus format) untuk autopilot/scheduler.
- [ ] `vac doctor --json` machine-readable output.

### 6.2 Failure mode catalog
- [ ] Dokumentasi `docs/RUNBOOK.md`: failure modes, diagnosis, recovery untuk setiap subsystem.
- [ ] Crash dump capture: panic hook menulis `~/.vac/crashes/<ts>.json` dgn redacted state.
- [ ] Telemetry opt-in (anonim) untuk crash rate.

### 6.3 Resource governance
- [ ] Memory cap per agent (configurable).
- [ ] Disk quota untuk `.vac/` (warn at 80%, block at 95%).
- [ ] Tool execution timeout enforcement audit (semua tool punya timeout).

### 6.4 Battle-test evidence
- [ ] Release stability period: 4 minggu tanpa critical bug pasca 1.0.
- [ ] ≥3 case study deployment internal terdokumentasi.
- [ ] Public changelog dengan zero retracted releases.

**Exit:** runbook lengkap, observability stack live di internal deployment, 4 minggu stability streak.

---

## Fase 7 — Benchmark Parity Polish (2–3 minggu) — *+3 poin*

Gap kecil tersisa untuk match feature-feature dari benchmark.

- [ ] LSP integration parity dgn OpenCode (multi-language).
- [ ] Desktop app shell (Tauri) — opsional tapi expected di benchmark.
- [ ] Plugin marketplace / rulebook registry.
- [ ] Web app surface (browse session history).

---

## Timeline ringkasan

| Fase | Durasi | Skor delta | Skor kumulatif |
|---|---:|---:|---:|
| 0 — Stabilisasi | 1–2 wk | 0 | 70 |
| 1 — Security primitives | 3–4 wk | +8.5 | 78.5 |
| 2 — Arch debt cleanup | 2–3 wk | +2.5 | 81 |
| 3 — Provider parity | 3–4 wk | +2.5 | 83.5 |
| 4 — Testing/CI | 2–3 wk | +4.8 | 88.3 |
| 5 — Release engineering | 4–6 wk | +15 | 90+ (capped by ops) |
| 6 — Production operability | 3–4 wk | +11.3 | 97 |
| 7 — Benchmark parity polish | 2–3 wk | +3 | **100** |

**Estimasi total: 19–29 minggu kerja terfokus** (~5–7 bulan dgn 1–2 engineer dedicated; ~3–4 bulan dgn tim 3–4).

---

## Critical path & dependencies

```
Fase 0 ──┬──► Fase 1 (security) ──┬──► Fase 5 (release/dist)
         │                         │
         └──► Fase 2 (arch debt) ──┴──► Fase 4 (CI) ──► Fase 6 (ops) ──► Fase 7
                                   │
                                   └──► Fase 3 (provider)
```

- **Fase 1 mem-block Fase 5**: tidak boleh distribusi binary dgn approval-forgery vector aktif.
- **Fase 4 mem-block Fase 6**: observability tanpa CI = telemetri tidak terverifikasi.
- **Fase 3 paralel** dengan Fase 2 dan Fase 4.

---

## Anti-pattern yang harus dihindari sepanjang roadmap

1. **Klaim tanpa verifikasi**: setiap exit criterion harus punya bukti yang bisa di-rerun (CI job, test name, artefak).
2. **Feature-flag perpetual**: feature-flag adalah tool migrasi, bukan strategi shipping. Fitur di balik flag >2 release = utang.
3. **Skip security utk speed**: setiap kali menambah primitive yang berlabel "secure"/"trusted", wajib ada threat model + adversarial test sebelum landed.
4. **Cloud-agent batch tanpa human review disiplin**: PR #15 menunjukkan batch besar dapat menyembunyikan regresi (`.cargo/config.toml`, false-secure primitives). Wajib per-subsystem audit untuk PR >2000 LOC.
5. **Stringly-typed contracts**: setiap `String` di config/API yang punya domain finit harus enum.

---

## Verdict gate

Skor 100/100 hanya diklaim setelah:
- Semua fase exit criteria terpenuhi.
- 3rd-party audit independen (re-engage Trae + audit kode VIL fresh).
- 4 minggu stability streak post-1.0.
- ≥4 distribution channel aktif dgn signed artefak.
- Provider matrix smoke hijau di ≥3 provider.
- Public security disclosure policy + threat model dipublikasikan.

Sebelum semua di atas terpenuhi, framing publik wajib tetap *"strong beta / near-production"* — bukan *"production-ready"*.
