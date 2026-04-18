# Implementation Plan V2 — Remaining Waves

**Tanggal dibuat**: 2026-04-18  
**Baseline**: branch `truth-reset-and-repo-hygiene`, 171 lib tests hijau, 0 clippy errors  
**Sumber kebenaran**: `docs/ROADMAP_TO_100_v2.md` · `docs/SUPERBATCH_PHASE_3_TO_7.md`  
**Prinsip**: verifikasi dulu apa yang sudah ada, compile dulu, test setiap fase, jangan invent scope.

---

## Status Aktual (Per 2026-04-18)

### Sudah selesai — JANGAN dikerjakan ulang

| Area | Bukti |
|---|---|
| D1a review patch engine | `crates/vac_cli/src/tui/services/review.rs`, test `git_stage_and_unstage_hunk_against_real_repo` |
| D1b shell session store | `ShellSessionStore` di `app/types.rs`, 4 handler tests |
| D2 search schema + providers + context pin | `vac_tools/src/builtin/search.rs`, test `symbol_provider_finds_pub_fn`, `recent_change_provider_returns_modified_files`, `pinned_context_visible_in_view` |
| Wave E TaskGraphProjection | `vac_core::engine::task_graph_projection()`, adapter wired |
| F0 VilIssue structured schema | `validation_issues: Vec<VilIssue>`, `VilSeverity`, `VilIssueKind` |
| F1 heatmap + conflict detector | `heatmap_by_file()`, `detect_rulebook_conflicts()`, 4 tests |
| H1.2 bundle fuzz | `crates/vac_core/fuzz/fuzz_targets/bundle_import.rs` |
| H1.3 policy gate fuzz | `crates/vac_core/fuzz/fuzz_targets/policy_gate.rs` |
| H1.4 THREAT_MODEL.md + SECURITY.md | `docs/THREAT_MODEL.md` lengkap dengan STRIDE table |
| H2.1 FSM proptest | `crates/vil_swarm/src/reasoning_fsm.rs` — proptest `property_fsm_*` |
| H2.2 RuntimeQueue boundary | `docs/RUNTIME_QUEUE_BOUNDARY.md` — decision: retain both |
| H2.3 LlmConfig single owner | `vac_core::config` re-export dari `vil_llm` |
| H3.1 engine zero concrete imports | `grep AnthropicProvider crates/vac_core/src/engine.rs` = 0 hits |
| H3.2 streaming parity | `crates/vil_llm/tests/provider_stream_parity.rs`, `PROVIDER_PARITY.md` |
| H3.3 config swap test | `crates/vac_core/tests/config_swap.rs` |
| CI infra (clippy, coverage, fuzz, mutations, CodeQL, cargo-deny) | `.github/workflows/{ci,coverage,fuzz-weekly,mutation-weekly,codeql,dependency-policy}.yml` |
| 6A.1 JSON structured logs | `--log-format json` di `telemetry.rs` |
| 6A.2 OpenTelemetry | `--otel-endpoint` di `telemetry.rs` |
| 6A.3 Prometheus metrics | `--metrics-addr` di `telemetry.rs` |
| 6A.4 crash capture | panic hook di `telemetry.rs` |
| RELEASING.md + RUNBOOK.md | `docs/RELEASING.md`, `docs/RUNBOOK.md` |
| cargo-dist initialized | `[workspace.metadata.dist]` di `Cargo.toml`, `release.yml` |
| vac migrate subcommand | `crates/vac_cli/src/commands/migrate.rs` |

---

## Yang Masih Harus Dikerjakan

### Validasi Gate Saat Ini

```bash
cargo check --workspace           # harus hijau
cargo clippy --workspace --all-targets -- -D warnings  # harus 0 errors
cargo test -p vac_cli --lib       # 171 tests
```

---

## Wave P0 — Bug Fixes Pending (Fase 0 ROADMAP_TO_100_v2)

### P0.1 — FSM SetRetry Bug

**File**: `crates/vil_swarm/src/orchestrator.rs`  
**Bug**: Line ~1342 memanggil `.apply(ReasoningEvent::SetRetry)` tanpa cek apakah iterasi berikutnya akan berjalan. Ini menyebabkan status reasoning melapor "Retry" untuk run yang sukses.

**Fix**:
```rust
// Sebelum emit SetRetry, cek apakah masih ada iterasi tersisa
if state.reasoning.attempt < state.reasoning.max_attempts {
    state.reasoning.apply(ReasoningEvent::SetRetry)?;
}
```

**Test yang harus ada**:
```rust
// crates/vil_swarm/src/orchestrator.rs (test module)
#[test]
fn successful_run_does_not_emit_set_retry()
// Assert: setelah run sukses (attempt == max_attempts), phase != Retry
```

**Gate**: `cargo test -p vil_swarm`

### P0.2 — Crash Dump Path Fix

**File**: `crates/vac_cli/src/telemetry.rs`  
**Bug**: panic hook menulis ke `cwd/crash_dump.json`. Roadmap mensyaratkan `~/.vac/crashes/<rfc3339-utc>.json`.

**Fix**: Ganti path di panic hook:
```rust
let crash_dir = dirs::home_dir()
    .unwrap_or_default()
    .join(".vac")
    .join("crashes");
let _ = std::fs::create_dir_all(&crash_dir);
let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
let dump_path = crash_dir.join(format!("{}.json", ts));
```

**Gate**: manual verify dump path; `cargo check -p vac_cli`

---

## Wave P1 — Security Hardening Gaps

### P1.1 — Policy Gate Corpus Expansion (30 → 100+ assertions)

**File**: `crates/vac_core/tests/policy_gate.rs`  
**Status saat ini**: 8 test functions, ~27 assertions. Roadmap mensyaratkan 100 bypass variant cases.

**Yang harus ditambah** — cover semua bypass variant berikut:
- `eval "rm -rf /"`, `eval $(cmd)`, backtick execution
- `bash -c "..."`, `sh -c "..."`, `zsh -c "..."`
- here-docs: `bash << 'EOF' ... EOF`
- `env VAR=val rm -rf`, `env -i bash`
- `nice/nohup/timeout/strace/gdb <cmd>`
- GNU parallel: `parallel rm ::: /etc /var`
- `xargs -I{} rm {}`, `find . -exec rm {} \;`
- `git -c alias.x=\!rm -rf / x`
- `kubectl-plugin_name` discovery
- `terraform -chdir=/tmp apply`
- Fish/zsh aliases dan funcs yang wrap gated commands

**Struktur**: gunakan data-driven dengan `&[(&str, bool)]` slice, iterate.

**Gate**: `cargo test -p vac_core --test policy_gate` — semua hijau; assertion count ≥ 100.

### P1.2 — Mutation Score Verification

**File**: `.cargo/mutants.toml` (sudah ada), `docs/audits/mutants-secret-detector.md`

Saat ini `.cargo/mutants.toml` ada tapi belum ada bukti score ≥80%.

**Tasks**:
1. Jalankan: `cargo mutants -p vac_core --file crates/vac_core/src/security/secret_detector.rs 2>&1 | tee docs/audits/mutants-secret-detector.md`
2. Untuk setiap surviving mutant, tambah test yang catches-nya.
3. Ulangi sampai score ≥80%.
4. CI `mutation-weekly.yml` harus memanggil cargo-mutants dan fail jika <80%.

**Gate**: `docs/audits/mutants-secret-detector.md` ada, score ≥80%; `mutation-weekly.yml` updated.

---

## Wave P2 — Property Test Expansion

### P2.1 — Bundle Proptest (idempotence + roundtrip)

**File**: `crates/vac_core/tests/bundle_roundtrip.rs`  
**Status**: 8 tests ada tapi bukan proptest.

**Tests baru yang harus ditambah**:
```rust
// Tambah ke bundle_roundtrip.rs atau file baru proptest_bundle.rs
#[cfg(test)]
mod proptest_bundle {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn bundle_export_import_roundtrip(
            summary in ".*",
            task_count in 0usize..10,
        ) {
            // build bundle, export, import, assert identical fields
        }
        
        #[test]  
        fn secret_detector_idempotence(input in ".*") {
            // detect(detect(x)) == detect(x) after substitution
        }
    }
}
```

**Gate**: `cargo test -p vac_core` hijau.

---

## Wave P3 — Release Engineering (TERBESAR yang tersisa)

Ini fase dengan impact terbesar. Banyak scaffolding sudah ada tapi belum complete.

### P3.1 — cargo-dist Target Lengkap + Installers

**File**: `Cargo.toml` bagian `[workspace.metadata.dist]`  

**Status saat ini**:
```toml
targets = [
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin", 
    "x86_64-pc-windows-msvc",
]
installers = []
```

**Yang harus ditambah**:
```toml
targets = [
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",      # static binary untuk Docker/Alpine
    "aarch64-unknown-linux-gnu",      # ARM server
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
]
installers = ["shell", "homebrew"]   # via cargo-dist
```

**Tasks**:
1. `cargo dist init` ulang dengan `--hosting=github` setelah update targets
2. Commit `dist-workspace.toml` yang di-generate
3. Verify `cargo dist plan` bisa berjalan tanpa error

**Gate**: `cargo dist plan --tag v0.1.0-rc1` exit 0; artifact list benar.

### P3.2 — Docker Image

**File baru**: `Dockerfile`, `.github/workflows/release.yml` (update)

```dockerfile
# Dockerfile
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build -p vac_cli --release

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /build/target/release/vac /usr/local/bin/vac
ENTRYPOINT ["/usr/local/bin/vac"]
```

**Tambah ke release.yml**:
```yaml
- name: Build and push Docker image
  uses: docker/build-push-action@v5
  with:
    context: .
    platforms: linux/amd64,linux/arm64
    push: true
    tags: ghcr.io/${{ github.repository_owner }}/vac:${{ github.ref_name }}
```

**Gate**: `docker build -t vac:local .` sukses; `docker run vac:local --version` output semver.

### P3.3 — install.sh Script

**File baru**: `install.sh`

Script minimal yang:
1. Detect OS/arch
2. Download tarball dari GitHub releases
3. Verify checksum
4. Install ke `$VAC_INSTALL_DIR` (default: `~/.local/bin`)
5. Idempotent (safe re-run)

```bash
#!/usr/bin/env bash
set -euo pipefail
VAC_VERSION="${VAC_VERSION:-latest}"
VAC_INSTALL_DIR="${VAC_INSTALL_DIR:-$HOME/.local/bin}"
# ... (platform detect, download, verify, install)
```

**Gate**: `bash install.sh --dry-run` menampilkan platform detection yang benar; `bash -n install.sh` (syntax check).

### P3.4 — Artifact Signing (Uncomment Release Pipeline)

**File**: `.github/workflows/release.yml`  
**Status**: minisign/cosign ada sebagai comment.

**Tasks**:
1. Generate dan commit `minisign.pub` (public key) ke repo
2. Store `MINISIGN_SECRET_KEY` di GitHub Secrets
3. Uncomment dan fix signing steps di `release.yml`
4. Add SBOM generation: `cargo cyclonedx --format json -o vac-sbom.json`

**Gate**: tagged RC release menghasilkan artifacts + `.minisig` files + SBOM; semua attached ke GitHub Release.

### P3.5 — Packaging Channels

**Prioritas**: Homebrew tap > AUR > Scoop

#### P3.5.1 Homebrew Tap
- Buat repo `vastar/homebrew-tap` (atau dokumentasikan untuk user)
- cargo-dist bisa auto-generate Homebrew formula
- Tambah ke `installers = ["shell", "homebrew"]` di Cargo.toml

#### P3.5.2 AUR PKGBUILD
**File baru**: `packaging/aur/PKGBUILD`

```
pkgname=vac-bin
pkgver=0.1.0
pkgrel=1
pkgdesc="VAC Agentic CLI"
arch=('x86_64')
url="https://github.com/vastar/vastar-agentic-cli"
license=('MIT')
source=("vac-${pkgver}-x86_64-linux.tar.gz::https://github.com/.../releases/download/v${pkgver}/...")
# ...
```

#### P3.5.3 Scoop Bucket
**File baru**: `packaging/scoop/vac.json`

**Gate per channel**: ≥4 channels functional → `vac --version` + `vac doctor` sukses.

### P3.6 — Schema Versioning di .vac/ Files

**Status**: `migrate.rs` ada tapi file schema belum include `schema_version` field.

**Files yang perlu update**:
- `crates/vac_core/src/session.rs` — tambah `pub schema_version: u32 = 1`
- `crates/vac_core/src/bundle.rs` — tambah `pub schema_version: u32 = 1`
- `crates/vac_core/src/queue.rs` — tambah `pub schema_version: u32 = 1`

**Migrasi**: jika `schema_version` tidak ada saat read → treat sebagai version 0, jalankan migrator.

**Test**:
```rust
// crates/vac_cli/tests/fixtures/legacy/ — simpan fixture .vac/ dari v0
#[test] fn legacy_compat_v0_session_loads()
#[test] fn legacy_compat_v0_queue_loads()
```

**Gate**: `cargo test -p vac_cli legacy_compat` hijau.

---

## Wave P4 — Operability Gaps

### P4.1 — Resource Governance (6A.5)

**File**: `crates/vac_tools/src/resource_limits.rs` (sudah ada), perlu wiring ke CLI config.

**Yang harus ditambah**:
- Field di `VacConfig`: `pub memory_cap_bytes: Option<u64>` dan `pub disk_quota_bytes: Option<u64>`
- Di `VacEngine::init()`: jika `memory_cap_bytes.is_some()`, panggil `apply_rlimit_as()`
- Di `VacEngine`: disk usage watcher yang warn di 80%, block write di 95%
- Tool timeout audit: setiap tool di `vac_tools::registry` harus punya explicit timeout field

**Test**:
```rust
#[test] fn engine_applies_memory_cap_from_config()
#[test] fn disk_quota_blocks_writes_at_95_percent()
```

**Gate**: `cargo test -p vac_tools -p vac_core`

### P4.2 — Trace Redaction Contract (6A.6)

**File**: `crates/vac_trace/src/redaction.rs` (sudah ada), perlu end-to-end enforcement.

**Yang harus diverifikasi** — setiap boundary berikut apply redaction:
1. `TraceRecorder::write()` sebelum flush ke disk
2. Bundle export (`bundle.rs`) — sudah ada?
3. Crash dump (panic hook di `telemetry.rs`) — apply redaction sebelum write
4. OTel export — apply redaction ke span attributes

**Test**:
```rust
// crates/vac_core/tests/trace_redaction.rs
#[test]
fn trace_containing_secrets_exports_zero_raw_secrets()
// Inject trace dengan setiap secret type dari H1.1 corpus
// Assert: exported trace/bundle tidak mengandung raw secrets
```

**Gate**: `cargo test -p vac_core --test trace_redaction`

---

## Wave P5 — Evidence Gate Preparation (6B)

Ini bukan engineering — tapi perlu persiapan dokumen dan tooling.

### P5.1 — CHANGELOG.md maintained

**File**: `CHANGELOG.md`  
Pastikan format conventional commits, tiap release punya entry, tidak ada yanking.

### P5.2 — Stability Log Bootstrap

**File**: `docs/STABILITY_LOG.md`  
Buat template entry harian yang akan diisi post-v1.0:
```markdown
## 2026-04-18
- Critical bugs: 0
- Known issues: [link ke GitHub Issues]
- Deploy status: internal-staging green
```

### P5.3 — Case Study Templates

**Dir**: `docs/case-studies/`  
Buat template `_template.md` untuk dokumentasi deployment internal.

---

## Urutan Implementasi yang Disarankan

```
P0.1  fix: FSM SetRetry emits only when next iteration will run
P0.2  fix: crash dump path → ~/.vac/crashes/<ts>.json
P1.1  feat: expand policy_gate corpus 30 → 100+ assertions  
P1.2  docs: run cargo-mutants, commit report, gate CI
P2.1  test: proptest bundle roundtrip + secret detector idempotence
P3.1  feat: cargo-dist config update (targets + installers)
P3.2  feat: Dockerfile + Docker CI step
P3.3  feat: install.sh script
P3.4  feat: enable artifact signing (minisign + SBOM)
P3.5  feat: packaging/aur/PKGBUILD + packaging/scoop/vac.json
P3.6  feat: schema_version field + legacy_compat tests
P4.1  feat: resource governance wiring (memory cap + disk quota)
P4.2  test: trace redaction end-to-end contract test
P5.1  docs: CHANGELOG.md cleanup
P5.2  docs: STABILITY_LOG.md template
P5.3  docs: case-studies/_template.md
```

---

## Validation Matrix

```bash
# Setelah setiap wave:
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p vac_cli --lib

# Setelah P1.x:
cargo test -p vac_core --test policy_gate

# Setelah P2.x:
cargo test -p vac_core

# Setelah P3.6:
cargo test -p vac_cli legacy_compat

# Setelah P4.x:
cargo test -p vac_tools -p vac_core --test trace_redaction

# Gate final sebelum v1.0 tag:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo dist plan --tag v1.0.0
```

---

## Risiko dan Guardrails

- **P3.x adalah engineering terbesar** — jangan mulai P3.2+ sebelum P3.1 (cargo-dist plan) berjalan
- **P3.6 schema migration** — gunakan dual-write window: baca N-1 dulu satu sprint sebelum hapus path lama
- **P1.1 policy gate corpus** — gunakan data-driven slice, jangan 100 test functions terpisah
- **P4.2 trace redaction** — jangan invent redaction baru; gunakan `vac_trace::redaction::RedactionPolicy` yang sudah ada
- **Jangan touch Phase 6B** (evidence gate) dari kode — itu observasi period, bukan engineering
- **Pre-existing failures** (`vac_core::tests::golden_*`) tetap out-of-scope

---

## Estimasi Waktu

| Wave | Estimasi |
|---|---|
| P0 (bug fixes) | 1–2 jam |
| P1 (security hardening) | 1–2 hari |
| P2 (proptest) | 4–8 jam |
| P3 (release engineering) | 3–5 hari |
| P4 (operability gaps) | 1–2 hari |
| P5 (evidence prep) | 2–4 jam |
| **Total** | **~2 minggu engineering** |
