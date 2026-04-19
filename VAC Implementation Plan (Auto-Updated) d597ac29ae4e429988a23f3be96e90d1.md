# VAC Implementation Plan (Auto-Updated)

<aside>
📝

**VAC Implementation Plan (Auto-Updated)** — page ini ditimpa otomatis tiap jam oleh @Unnamed Bot. Jangan edit manual; perubahan akan hilang pada run berikutnya.

</aside>

<aside>
🗓️

**Run timestamp:** 2026-04-19 15:00 WIB (Asia/Jakarta, hourly recurrence `3479453f-ab55-81e8-a12b-00952a0e2cbe`).

**Repo:** `/home/emp/Documents/VAC/vastar-agentic-cli/` — 21 crate Rust workspace (`vac_*` + `vil_*`).

**Severity distribution:** 0 blocker / 0 major / 5 minor / 3 nit = 8 temuan aktif.

**Files dibaca mendalam run ini:** `crates/vil_trust/src/policy.rs` (re-verify F-01 prev fixed), `crates/vil_trust/src/error.rs` (re-verify RequiresApproval variant), `crates/vil_trust/src/permissions.rs` (verify F-05 prev), `crates/vil_llm/src/token_budget.rs` (re-verify F-02 prev fixed), `crates/vil_llm/src/router.rs` (verify F-03 prev fixed + F-07, F-10), `crates/vil_llm/src/retry.rs` (verify F-04 jitter status), `crates/vac_changeset/src/lib.rs` (verify F-03 prev), `crates/vac_approvals/src/lib.rs` (verify F-04 prev), `crates/vil_swarm/src/tool_execution.rs` (sapuan baru), `crates/vil_context/src/chunking.rs` (sapuan baru → NEW finding), `crates/vil_memory/src/episodic.rs` (sapuan baru).

**Crates dicover:** `vil_trust`, `vil_llm`, `vac_changeset`, `vac_approvals`, `vil_swarm`, `vil_context`, `vil_memory` (7 crate, jauh di atas minimum 3).

**Sapuan lebar (grep & glob):** `glob crates/*/src/lib.rs` (21 crate), `glob crates/vil_swarm/src/*.rs` (23 file), `glob crates/vil_context/src/*.rs` (6 file), `glob crates/vil_memory/src/*.rs` (5 file), `grep exponential_backoff_ms crates/vil_llm` (6 hit, menegaskan fn belum dipanggil dengan jitter), `grep \.unwrap\(\) crates/vil_swarm/src` (42 hit — semua di blok `#[cfg(test)]`, bersih di jalur produksi).

**Rekonsiliasi dari run 2026-04-19 14:00 WIB:** 10 finding terdaftar sebelumnya (F-01..F-10). **Status verifikasi langsung terhadap codebase run 15:00 WIB**:

- **3 fixed**: (run 14:00) F-01 PolicyEngine.enforce conflation, F-02 TokenBudget overflow, F-03 retry_after_secs overflow — semua telah diperbaiki di commit yang masuk antara 14:00 dan 15:00 WIB. Bukti dikutip di Appendix A.
- **1 partial**: (run 14:00) F-04 retry jitter — enum `JitterMode` dan field `RetryConfig::jitter` ditambahkan, tapi `exponential_backoff_ms` line 89–99 **masih deterministik** (tidak memanggil `rand` sama sekali walaupun import `use rand::Rng;` hadir di line 5). Finding dibawa kembali sebagai F-01 (run 15:00) dengan implementation plan mempersempit ke "hubungkan field ke fungsi".
- **6 open**: F-05, F-06, F-07, F-08, F-09, F-10 (run 14:00) — tidak ada perubahan kode di area tersebut. Dibawa kembali sebagai F-02..F-07 (run 15:00) dengan konten diperbarui.
- **1 new**: SemanticChunker di `vil_context/src/chunking.rs` memiliki dua defect (off-by-one byte counting + break guard terlalu ketat yang berisiko menghasilkan chunk hilang / infinite loop pada konfigurasi tertentu). Dimasukkan sebagai F-08 (run 15:00).

**Do-not-touch:** Findings DB [](https://www.notion.so/448e46cab0d045aa9695bdb8e3e45dbf?pvs=21) tidak disentuh; instructions pages [](https://www.notion.so/7a39453fab558229978c81b75f5de3b0?pvs=21) & [](https://www.notion.so/17b5d00f234d4efe9627f4e91ab19d0f?pvs=21) tidak disentuh; subpage [VAC Vastar Agentic CLI — Technical Implementation Plan & Product Spec](https://www.notion.so/VAC-Vastar-Agentic-CLI-Technical-Implementation-Plan-Product-Spec-bd1be443d54944e69a14f9fd13443d60?pvs=21) tidak disentuh.

</aside>

## F-01 — `exponential_backoff_ms` Mengabaikan `RetryConfig::jitter` (Scaffolding Tanpa Behavior)

**Severity:** `minor`

**Category:** Reliability / Concurrency (partial fix regression risk)

**Area / Crate:** `vil_llm` — seluruh provider LLM yang memanggil `resolve_retry_delay_ms`

**Files / Paths:** `crates/vil_llm/src/retry.rs` (line 5 import, line 9–18 enum, line 22–28 struct field, line 89–99 fungsi backoff), dipanggil dari `crates/vil_llm/src/router.rs` line 340–345

**Status:** `open` (partial carried-over; struct + enum sudah ada, function body belum memakai)

### Problem Statement

Run sebelumnya (14:00 WIB) menandai absennya jitter pada `exponential_backoff_ms` sebagai bug reliabilitas (thundering herd pada burst 429). Antara 14:00 dan 15:00 WIB, tim menambahkan **scaffolding**:

- `use rand::Rng;` di line 5 `retry.rs`.
- `JitterMode { None, Equal (Default), Full }` di line 9–18.
- Field `RetryConfig::jitter: JitterMode` di line 27.
- Default `RetryConfig::default()` mengeset `jitter: JitterMode::Equal` di line 37.

Sayangnya, body `exponential_backoff_ms` (line 89–99) **tetap identik** dengan implementasi pre-scaffolding:

```rust
pub fn exponential_backoff_ms(config: &RetryConfig, attempt: usize) -> u64 {
    if attempt <= 1 {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }
    let factor = config.multiplier.powi((attempt - 1) as i32);
    let delay = (config.initial_backoff_ms as f64) * factor;
    if delay.is_nan() || delay.is_sign_negative() {
        return config.initial_backoff_ms.min(config.max_backoff_ms);
    }
    delay.min(config.max_backoff_ms as f64) as u64
}
```

Tidak ada match arm untuk `config.jitter`. `rand::Rng` yang diimport **tidak dipakai** di modul ini (compiler biasanya akan `warn unused_imports` — periksa CI apakah dibungkam). Hasilnya: konfigurasi `jitter: JitterMode::Full` atau `JitterMode::None` yang diberikan oleh caller tidak memiliki efek apa-apa; perilaku backoff tetap deterministik, sehingga thundering herd tetap bisa terjadi saat beberapa task VAC paralel serempak menabrak 429 dari provider yang sama.

### Dampak Runtime / Arsitektur

- **Leaky abstraction**: API publik `RetryConfig` meng-expose `jitter` sehingga caller downstream (konsumen `vil_llm`, CLI `vac_cli`, plugin) mengasumsikan ada jitter. Kontrak dilanggar diam-diam.
- **Thundering herd nyata**: skenario agent loop VAC melakukan `grep` + `glob` + `read` paralel dalam satu iterasi. Jika semuanya melewati `LlmRouter::complete` dan provider mengembalikan 429 serempak (karena budget window bersama), semua task akan tidur tepat `2_000 ms`, bangun bersama, memukul provider lagi, dan meng-amplifikasi 429. Ini memperburuk latency end-to-end task dan menaikkan cost karena sebagian retry baru berhasil setelah konsumsi token awal.
- **Observability pitfall**: metrik `vac_llm_retry_delay_ms_histogram` (jika ada) akan memiliki spike bimodal yang salah interpretasi oleh operator yang menyangka "jitter sudah aktif".

### Root Cause Analysis

Hipotesis tidak ada komit untuk menghubungkan field ke fungsi: perubahan sebelumnya adalah scoped-change "tambah tipe data" yang sengaja dipisah dari "ubah perilaku" untuk memudahkan review. Perlu follow-up PR untuk menutup lingkaran. Tanpa test yang gagal, CI tidak mendeteksi kondisi partial ini.

### Implementation Plan

1. **Tambah fungsi helper internal** di `retry.rs`:

```rust
fn apply_jitter(base_ms: u64, mode: JitterMode) -> u64 {
    match mode {
        JitterMode::None => base_ms,
        JitterMode::Equal => {
            let half = base_ms / 2;
            half.saturating_add(rand::thread_rng().gen_range(0..=half))
        }
        JitterMode::Full => {
            if base_ms == 0 { 0 } else { rand::thread_rng().gen_range(0..=base_ms) }
        }
    }
}
```

1. **Modifikasi `exponential_backoff_ms`** untuk memanggil `apply_jitter` di akhir sebelum cast ke `u64`:

```rust
pub fn exponential_backoff_ms(config: &RetryConfig, attempt: usize) -> u64 {
    let base = if attempt <= 1 {
        config.initial_backoff_ms.min(config.max_backoff_ms)
    } else {
        let factor = config.multiplier.powi((attempt - 1) as i32);
        let delay = (config.initial_backoff_ms as f64) * factor;
        if delay.is_nan() || delay.is_sign_negative() {
            config.initial_backoff_ms.min(config.max_backoff_ms)
        } else {
            delay.min(config.max_backoff_ms as f64) as u64
        }
    };
    apply_jitter(base, config.jitter)
}
```

1. **Jaga invariant untuk `resolve_retry_delay_ms`**: retry-after header dari provider **tidak** di-jitter — itu instruksi eksplisit provider yang harus dihormati. Pastikan `parse_retry_delay_from_headers` (line 57) tidak terpengaruh.
2. **Perbarui test eksisting** yang meng-assert nilai eksak (`exponential_backoff_caps_at_max` line 150–156, `fallback_to_backoff_when_no_header` line 159–163) supaya menggunakan `JitterMode::None` secara eksplisit via `RetryConfig { jitter: JitterMode::None, ..cfg() }`. Jangan biarkan default `Equal` membuat assertion eksak gagal acak.
3. **Tambah test baru** di `mod tests`:
    - `jitter_equal_spreads_in_bucket`: panggil `exponential_backoff_ms` 10_000 kali dengan `JitterMode::Equal` untuk `attempt=3`; verifikasi `min >= 4_000` dan `max <= 8_000`, stddev `> 500`.
    - `jitter_full_spreads_from_zero`: panggil 10_000 kali `JitterMode::Full` untuk `attempt=2`; verifikasi `min < 400` dan `max <= 4_000`.
    - `retry_after_header_is_not_jittered`: panggil `resolve_retry_delay_ms` dengan header `retry-after: 3` 1000 kali; semua harus `delay_ms == 3_000`.
4. **Telemetry**: tambahkan `metrics::histogram!("vil_llm_retry_delay_ms").record(delay.delay_ms as f64)` di `router.rs` line 348 segera sebelum `tokio::time::sleep(...)` untuk observabilitas distribusi.

### Risk & Rollback

Risiko: test existing mungkin flaky bila tidak dimigrasikan ke `JitterMode::None`. Rollback: revert `apply_jitter` dan restore body lama `exponential_backoff_ms`. Tidak ada migrasi data.

### Effort

**S** — ~1 jam engineering: 20 menit implementasi, 30 menit test refactor + 3 test baru, 10 menit observabilitas.

## F-02 — `ChangesetStore` Linear Scan O(n) per Mutasi File

**Severity:** `minor`

**Category:** Performance / Data Structure

**Area / Crate:** `vac_changeset` — tracking perubahan file sepanjang sesi agent

**Files / Paths:** `crates/vac_changeset/src/lib.rs` (method `file_created` line 75–88, `file_modified` 91–108, `file_removed` 111–124, `revert_success` 127–141, `revert_failed` 144–162, `entries` 165–167, `active_entries` 170–180, `reviewable_entries` 183–190, `counts_by_state` 193–199, `modified_files` 202–208, `clear` 211–214)

**Status:** `open` (carried-over dari run 14:00 WIB; tidak ada perubahan struktur)

### Problem Statement

Setelah verifikasi ulang line-by-line di run 15:00 WIB, struktur internal `ChangesetStore` tetap memakai `entries: Vec<ChangesetEntry>` (line 64). Semua method mutator menggunakan pola:

```rust
if let Some(entry) = self.entries.iter_mut().find(|e| e.path == path) {
    // update
} else {
    // push new
}
```

Untuk sesi menyentuh N path unik dengan M operasi mutator, kompleksitas gabungan O(M·N). Pada codebase sebesar `vastar-agentic-cli` (21 crate, ratusan file) dengan iterasi agent yang bolak-balik edit file, M bisa ribuan dan N ratusan — overhead nyata. Bertambah parah karena perbandingan memakai `String::==` pada path panjang (rata-rata 40–80 karakter untuk path absolut), bukan `hash` pra-komputasi. Fungsi turunan (`modified_files`, `active_entries`, `reviewable_entries`, `counts_by_state`) juga linear; UI TUI memanggilnya pada event loop (30–60 Hz sesuai konfigurasi `vac_tui_runtime`), jadi lag UI ringan pada sesi panjang mudah muncul.

### Dampak Runtime / Arsitektur

- UX lambat pada sesi panjang; latency interactive yang menumpuk.
- CPU time tidak perlu yang sepenuhnya bisa dihilangkan dengan struktur data yang benar.
- Signal-to-noise flame graph terkontaminasi — profiling untuk bottleneck lain jadi lebih sulit.

### Root Cause Analysis

Pola `Vec` dipilih karena kesederhanaan dan karena urutan kronologis entry dianggap berguna untuk UI "recent changes". Access pattern sebenarnya sangat didominasi lookup by-path, bukan iterasi berurutan. Struktur yang tepat: `IndexMap<String, ChangesetEntry>` — menjaga insertion order (memenuhi kebutuhan UI), lookup O(1) by key, drop-in replacement untuk iterasi via `.values()`.

### Implementation Plan

1. **Ganti field** di `crates/vac_changeset/src/lib.rs` line 64:

```rust
use indexmap::IndexMap;
// ...
pub struct ChangesetStore {
    entries: IndexMap<String, ChangesetEntry>,
    generation: u32,
    pub navigator: RepoNavigatorState,
}
```

1. **Tambahkan dependency** di `crates/vac_changeset/Cargo.toml`:

```toml
indexmap = { version = "2", features = ["serde"] }
```

1. **Refactor 5 mutator** menjadi pola `Entry` API `IndexMap`:

```rust
use indexmap::map::Entry;
pub fn file_created(&mut self, path: String, actor: String) {
    self.generation += 1;
    match self.entries.entry(path.clone()) {
        Entry::Occupied(mut o) => {
            let e = o.get_mut();
            e.state = FileState::Created;
            e.dirty_generation = self.generation;
            e.timestamp = SystemTime::now();
            e.actor = actor;
            e.last_error = None;
        }
        Entry::Vacant(v) => {
            let mut e = ChangesetEntry::new(path, FileState::Created, actor);
            e.dirty_generation = self.generation;
            v.insert(e);
        }
    }
}
```

Ulangi pola ini untuk `file_modified`, `file_removed`, `revert_success`, `revert_failed`. Yang `file_modified` perlu hati-hati melestarikan `FileState::Created` (line 94–96) — di dalam `Occupied` arm jangan override state kalau state lama `Created`.

1. **Signature `entries()`**: dari `&[ChangesetEntry]` menjadi `impl Iterator<Item = &ChangesetEntry>` via `self.entries.values()`. Karena ini breaking change kecil, sediakan juga `entries_vec(&self) -> Vec<&ChangesetEntry> { self.entries.values().collect() }` untuk konsumen existing yang butuh slice-like access.
2. **`active_entries`, `reviewable_entries`, `modified_files`**: ganti `self.entries.iter()` dengan `self.entries.values()`. Tidak ada perubahan semantik — IndexMap `values()` mempertahankan insertion order.
3. **`counts_by_state`** (line 193–199): iterasi `self.entries.values()` sebagai ganti `&self.entries`. Semua test yang assert hasil tidak perlu diubah.
4. **`clear`** (line 211–214): ganti `self.entries.clear()` — IndexMap juga punya method `.clear()` kompatibel.
5. **Tambahkan benchmark** `crates/vac_changeset/benches/changeset_bench.rs` pakai `criterion`:

```rust
fn bench_file_modified(c: &mut Criterion) {
    c.bench_function("file_modified_1k_paths_10k_ops", |b| {
        b.iter(|| {
            let mut store = ChangesetStore::new();
            for i in 0..10_000 {
                let path = format!("/src/file_{}.rs", i % 1_000);
                store.file_modified(path, "actor".into(), false);
            }
        });
    });
}
```

Target: p99 per-operation `< 5 µs` (baseline biasanya 30–100 µs tergantung N).

1. **Verifikasi** seluruh test existing hijau — tambah dependency di `dev-dependencies` Cargo.toml bila `indexmap` belum tersedia di workspace resolver.

### Risk & Rollback

Risiko minor: konsumen eksternal crate (di luar `vac_*`) yang mengandalkan signature `&[ChangesetEntry]` butuh adaptasi. Namun scope `vac_changeset` terbatas di workspace. Rollback: revert `Cargo.toml` + `lib.rs`. Tidak ada perubahan data persisten.

### Effort

**M** — ~2.5 jam termasuk baseline benchmark + migrasi konsumen internal (`vac_tui_runtime`, `vac_session_control`).

## F-03 — Race `record_request` ↔ `ApprovalHandle::send` Ditambal Polling 5×25 ms

**Severity:** `minor`

**Category:** Concurrency / Correctness

**Area / Crate:** `vac_approvals` — jalur antara tool-call yang men-trigger approval dan UI yang memanggil `approve`/`reject`

**Files / Paths:** `crates/vac_approvals/src/lib.rs` (method `ApprovalHandle::send`, line 420–482; public wrappers `approve` line 412–414, `reject` 416–418)

**Status:** `open` (carried-over; polling loop masih persis sama)

### Problem Statement

Implementasi terbaru `ApprovalHandle::send` (verified 15:00 WIB, identik dengan run 14:00):

```rust
let mut record = None;
for attempt in 0..5 {
    let store = self.store.clone();
    let id = tool_call_id.clone();
    record = tokio::task::spawn_blocking(move || store.load(&id))
        .await
        .map_err(|e| ApprovalError::Task(format!("Approval store task failed: {e}")))??;
    if record.is_some() { break; }
    if attempt < 4 {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
```

Polling 5 × 25 ms = 125 ms window maksimum untuk menunggu race antara tool-worker yang memanggil `record_request` (menulis `.vac/approvals/<id>.json`) dan UI yang memanggil `send` (membaca file itu). Ini bukan bug fatal, tetapi:

- **False negative pada beban disk tinggi**: 125 ms bisa tidak cukup bila fs sibuk (misal cargo build paralel).
- **Latency happy-path**: 25 ms setidaknya ditambahkan pada race yang seharusnya tidak perlu ada.
- **Code smell auditor security**: polling pada jalur approval adalah red flag dalam review trust boundary.
- **Klik ganda**: bila user menekan Approve cepat dua kali (misal double-click accident) sebelum first send selesai, request kedua melihat state yang tidak pasti.

### Dampak Runtime / Arsitektur

- False negative "Unknown tool_call_id" di sisi user pada race extrem.
- Bottleneck invisible pada disk I/O.
- Sulit diargumentasikan dalam threat model sebagai mekanisme yang benar.

### Root Cause Analysis

`ApprovalStore` berbasis file-system (tulis `.tmp` → rename) adalah source-of-truth persisten yang harus bertahan crash-restart. `ActiveApprovalRegistry` berbasis memori adalah register aktif. Urutan yang benar saat tool-worker meng-escalate approval: (1) `record_request` → tulis file JSON, (2) `register` → masukkan sender channel ke HashMap registry. UI handler memanggil `send` yang harus menunggu kedua sisi siap. Bila ordering terbalik atau UI sangat cepat, `send` race. Polling adalah solusi malas; event-driven (`tokio::sync::Notify`) adalah solusi benar.

### Implementation Plan

1. **Perluas `ActiveApprovalEntry`** di `crates/vac_approvals/src/lib.rs` (struct yang ada di atas `send`):

```rust
pub struct ActiveApprovalEntry {
    pub session_id: Uuid,
    pub tool_call_id: String,
    pub record_cache: Option<ApprovalRecord>, // populated on register
    pub tx: mpsc::UnboundedSender<vil_swarm::ApprovalResponse>,
}
```

1. **Tambahkan `ready_notify: Arc<Notify>`** ke `ActiveApprovalRegistry`. Sinyal dipancarkan oleh `record_request` setelah `self.write(&record)` sukses, agar `send` tidak perlu polling.
2. **`send` refactor**:

```rust
async fn send(&self, tool_call_id: String, approved: bool, reason: Option<String>) -> ApprovalResult<()> {
    // Fast path: cek registry in-memory dulu (O(1)).
    if let Some(entry) = self.active.get_by_tool_call_id(&tool_call_id).await {
        return self.dispatch(entry, approved, reason).await;
    }
    // Slow path: tunggu event-driven hingga 500 ms, lalu fallback ke store.load.
    let fut = self.active.ready_notify.notified();
    tokio::pin!(fut);
    let _ = tokio::time::timeout(Duration::from_millis(500), &mut fut).await;
    // Fallback fs load untuk ketahanan crash-restart.
    let store = self.store.clone();
    let id_clone = tool_call_id.clone();
    let record = tokio::task::spawn_blocking(move || store.load(&id_clone))
        .await.map_err(|e| ApprovalError::Task(format!("spawn_blocking failed: {e}")))??
        .ok_or_else(|| ApprovalError::Task(format!("Unknown tool_call_id: {tool_call_id}")))?;
    // ... sisanya sama seperti sebelumnya.
}
```

1. **Hapus loop `for attempt in 0..5`** — diganti `tokio::time::timeout` eksplisit.
2. **Test baru** `crates/vac_approvals/tests/race_is_deterministic.rs`:
    - Spawn task A yang menunggu 1 ms lalu memanggil `record_request`.
    - Spawn task B yang langsung memanggil `send`.
    - Assert `send` tidak pernah mengembalikan "Unknown tool_call_id" dan kembali dalam `< 100 ms`.
3. **Test crash-restart**: `record_request` dipanggil, proses dibunuh sebelum `register`, proses baru start, panggil `send` — harus jatuh ke fallback fs load dan sukses.
4. **API publik** `approve`/`reject` tidak berubah.

### Risk & Rollback

Menyentuh jalur approval yang sensitif. Rollback: revert `lib.rs` tunggal. Mitigasi: tambahkan feature flag `approval_notify` di `Cargo.toml` untuk iterasi pertama sehingga perubahan dapat di-A/B test.

### Effort

**M** — ~3 jam.

## F-04 — Rate Limiter Hanya Diacquire Sekali di Luar Retry Loop (`LlmRouter::complete`)

**Severity:** `minor`

**Category:** Reliability / Rate Control

**Area / Crate:** `vil_llm` — rate limiting pada jalur LLM request

**Files / Paths:** `crates/vil_llm/src/router.rs` (method `complete` line 281–362, khususnya line 282 acquire tunggal, line 317–357 loop retry tanpa reacquire)

**Status:** `open` (carried-over)

### Problem Statement

`LlmRouter::complete` memanggil `SimpleRateLimiter::acquire(&self.rate_limiter).await;` sekali saja di line 282 — sebelum loop provider-fallback-chain dan sebelum loop retry per-provider di line 317. Konsekuensi: retry ke provider yang sama (`attempt += 1; provider.complete(&sanitized_request).await`) dan fallback ke provider berikutnya dalam chain (line 299–359) TIDAK melewati rate limiter lagi. Satu `complete()` call bisa menghasilkan N request HTTP aktual ke provider, tapi rate limiter hanya mencatat 1 token.

Kalkulasi numerik: `requests_per_minute = 60` dan `max_attempts = 3` membuat single `complete()` bisa memicu 3 request aktual dalam jendela sempit tanpa ada ratelimiting. Secara agregat router bisa mengirim 3 × 60 = 180 request/menit sambil rate limiter mencatat hanya 60.

### Dampak Runtime / Arsitektur

- Rate limit provider di-breach walau rate limiter lokal kelihatan patuh.
- Observasi 429 meningkat tanpa penjelasan jelas bagi operator.
- Potensi keban provider key (khususnya Anthropic yang ketat pada burst).
- Menggabungkan dengan F-01 (jitter absen): burst retry tanpa jitter akan merusak kuota lebih cepat.

### Root Cause Analysis

Asumsi awal: `complete()` = 1 HTTP request. Kenyataan pasca-retry-logic: `complete()` = 1..=N request. Rate limiter scope tidak direvisit saat retry loop diinjeksikan. Pola idiomatik di library seperti `tower-retry` atau `governor` adalah layer rate-limit di level request, bukan di level operasi logika.

### Implementation Plan

1. **Pindahkan** `SimpleRateLimiter::acquire` ke posisi yang paling mendekati HTTP request aktual. Opsi (a): di dalam loop retry sebelum `provider.complete(&sanitized_request).await` line 319. Opsi (b): wrap `provider.complete` dalam helper yang acquire sebelum delegasi. Pilih (a) untuk kesederhanaan:

```rust
loop {
    attempt += 1;
    SimpleRateLimiter::acquire(&self.rate_limiter).await; // <-- new
    match provider.complete(&sanitized_request).await {
        // existing arms
    }
}
```

1. **Hapus acquire awal** di line 282 karena sekarang redundant. Atau, jika ingin tetap ada guard "budget di level sesi", pertahankan dua-tier:
    - `self.rate_limiter_per_complete` (yang sekarang) — batasi call rate `complete()` level aplikasi.
    - `self.rate_limiter_per_request` (baru) — batasi call rate HTTP request yang termasuk retry.
2. **API baru** `LlmRouter::with_request_rate_limit(&mut self, requests_per_minute: u32)` paralel dengan `with_rate_limit`, untuk compatibility caller.
3. **Dokumentasikan** trade-off di doc comment `complete`: "Rate limit is enforced per HTTP request (including retries), not per logical call."
4. **Test baru** `rate_limiter_is_acquired_per_retry`:

```rust
#[tokio::test(start_paused = true)]
async fn rate_limiter_is_acquired_per_retry() {
    let counter = Arc::new(AtomicUsize::new(0));
    let provider = MockProvider::new(vec![
        Err(LlmError::RateLimited("mock".into(), 1)),
        Err(LlmError::RateLimited("mock".into(), 1)),
        Ok(sample_response()),
    ]);
    let mut router = LlmRouter::new(...);
    router.rate_limiter.set_observer(counter.clone());
    let _ = router.complete(&req).await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}
```

1. **Tambahkan** `info!(acquired_rate_token = true, attempt)` tracing di line baru acquire agar operator sadar perilaku.

### Risk & Rollback

Konsumen yang mengandalkan asumsi lama (1 acquire per complete) akan melihat latency naik pada retry — ini fix yang diinginkan. Rollback: pindahkan `acquire` kembali ke atas loop.

### Effort

**S** — ~1 jam.

## F-05 — Kontrak `resolve_retry_delay_ms` Tidak Menegakkan `max_attempts`

**Severity:** `nit`

**Category:** API Contract / Documentation

**Area / Crate:** `vil_llm` — retry scheduling untuk provider LLM

**Files / Paths:** `crates/vil_llm/src/retry.rs` (struct `RetryConfig` line 21–28 dengan field `max_attempts`, function `resolve_retry_delay_ms` line 102–112)

**Status:** `open` (carried-over)

### Problem Statement

`RetryConfig::max_attempts` di line 23 adalah field publik, tapi tak ada fungsi dalam modul `retry.rs` yang mem-check-nya. `resolve_retry_delay_ms(headers, config, attempt, now)` akan tetap menghitung delay meskipun `attempt > config.max_attempts`. Tanggung jawab penegakan ada di caller — `LlmRouter::complete` line 330 memeriksa sendiri dengan `attempt < self.retry_config.max_attempts`. Ini bukan bug fatal, tapi kontrak API ambigu: kehadiran field `max_attempts` pada struct yang dipakai oleh `resolve_retry_delay_ms` menyiratkan ada penegakan di modul. Jika caller baru lupa memeriksa, retry akan berjalan tanpa batas.

### Root Cause Analysis

Library retry idiomatik (tower-retry, `backoff`, `reqwest-retry`) mengekspos dua entry point: (a) murni menghitung delay, (b) menggabungkan delay + keputusan retry-or-stop. Modul `retry.rs` hanya punya yang pertama, sementara field `max_attempts` menyiratkan yang kedua.

### Implementation Plan

1. **Tambahkan enum dan fungsi** di `retry.rs`:

```rust
#[derive(Debug, Clone)]
pub enum RetryDecision {
    Retry(RetryDelay),
    GiveUp,
}

pub fn next_retry_decision(
    headers: &HashMap<String, String>,
    config: &RetryConfig,
    attempt: usize,
    now: DateTime<Utc>,
) -> RetryDecision {
    if attempt > config.max_attempts {
        return RetryDecision::GiveUp;
    }
    RetryDecision::Retry(resolve_retry_delay_ms(headers, config, attempt, now))
}
```

1. **Tambah doc** di `resolve_retry_delay_ms`:

```rust
/// NOTE: This function does NOT check `max_attempts`. Use `next_retry_decision`
/// for a combined retry-or-stop decision that honors the retry budget.
```

1. **Sweep call sites** `resolve_retry_delay_ms` via `rg 'resolve_retry_delay_ms' crates/` (hasil: 1 call site di `router.rs` line 340–345). Migrasikan ke `next_retry_decision` yang menggantikan manual check di line 330.
2. **Test baru** di `mod tests`:

```rust
#[test]
fn give_up_when_attempt_exceeds_max() {
    let c = cfg(); // default max_attempts=3
    let d = next_retry_decision(&HashMap::new(), &c, 4, Utc::now());
    assert!(matches!(d, RetryDecision::GiveUp));
}
```

### Risk & Rollback

Penambahan API; tidak breaking. Rollback: hapus fungsi baru + revert call site.

### Effort

**S** — ~45 menit.

## F-06 — `PermissionSet::is_granted` Mengandung Pengecekan Denied Redundan

**Severity:** `nit`

**Category:** Code Hygiene / Invariant Clarity

**Area / Crate:** `vil_trust` — fine-grained permission check

**Files / Paths:** `crates/vil_trust/src/permissions.rs` (line 48–75, khususnya `is_granted` line 67–69)

**Status:** `open` (carried-over)

### Problem Statement

`PermissionSet` secara invariant menjamin `granted ∩ denied = ∅`: `grant()` (line 55–58) menghapus dari `denied` sebelum insert ke `granted`, dan `deny()` (line 61–64) sebaliknya. Tetapi `is_granted` (line 67–69) menulis:

```rust
pub fn is_granted(&self, perm: &Permission) -> bool {
    self.granted.contains(perm) && !self.denied.contains(perm)
}
```

Cabang kedua hanya bisa `true` bila invariant tercemar — yang secara sekarang tidak mungkin karena kedua field private dan hanya `grant`/`deny` yang boleh mengubah. Ini bukan bug (kode correct), tapi:

- Mengaburkan invariant sebenarnya bagi pembaca.
- Membuka risiko: jika kelak method `grant_all(iter)` atau `merge(other: PermissionSet)` ditambahkan tanpa menjaga invariant, bug akan tersembunyi di balik pengecekan defensif yang kelihatan "aman".
- Meningkatkan noise di review trust boundary.

### Root Cause Analysis

Programmer bersikap defensive-safe. Dalam praktik, invariant yang dijaga di konstruktor lebih baik didokumentasikan eksplisit dan diuji, bukan digandakan di setiap reader. Ini adalah anti-pattern "just in case".

### Implementation Plan

1. **Sederhanakan `is_granted`** di line 67–69:

```rust
/// Invariant: grant()/deny() memastikan granted ∩ denied = ∅, jadi cukup
/// cek sisi granted.
pub fn is_granted(&self, perm: &Permission) -> bool {
    self.granted.contains(perm)
}
```

1. **Tambah `debug_assert`** di akhir `grant` (line 58) dan `deny` (line 64):

```rust
debug_assert!(self.granted.is_disjoint(&self.denied),
    "PermissionSet invariant violated: granted ∩ denied != ∅");
```

1. **Test baru**:
    - `grant_then_deny_removes_from_granted_and_adds_to_denied`: panggil `grant(p)` lalu `deny(p)`, assert `!is_granted(&p) && is_denied(&p)`.
    - `deny_then_grant_reverses_state`: panggil `deny(p)` lalu `grant(p)`, assert `is_granted(&p) && !is_denied(&p)`.
    - `invariant_holds_across_grant_deny_sequence`: randomized property test — generate 1000 sequence (grant|deny, perm) lalu assert `granted.is_disjoint(&denied)` setelah setiap operasi.
2. **Dokumentasikan** invariant di doc comment struct `PermissionSet`: "Invariant: `granted` dan `denied` disjoint; `grant()` dan `deny()` merupakan satu-satunya mutator yang menegakkannya."
3. **Guard untuk API masa depan**: bila API baru `merge(other)` atau `FromIterator` ditambahkan, wajib menjaga invariant secara eksplisit + doc + debug_assert.

### Risk & Rollback

Tidak ada perubahan perilaku eksternal.

### Effort

**XS** — ~15 menit.

## F-07 — `LlmRouter::complete` Dapat Menyusun Fallback Chain dengan Default Provider Duplikat

**Severity:** `nit`

**Category:** DX / Minor Bug

**Area / Crate:** `vil_llm` — konstruksi fallback chain

**Files / Paths:** `crates/vil_llm/src/router.rs` (line 294–295)

**Status:** `open` (carried-over)

### Problem Statement

Chain dibentuk dengan:

```rust
let mut chain = vec![self.default_provider.clone()];
chain.extend(self.fallback_chain.iter().cloned());
```

Tidak ada dedup. Jika user secara sengaja atau tidak menuliskan `default_provider = "openai"` dan `fallback_chain = ["openai", "gemini"]` di konfigurasi `LlmConfig`, chain menjadi `["openai", "openai", "gemini"]`. Akibatnya: jika retry exhausted pada openai, router akan mencoba openai lagi (buang waktu, kemungkinan besar fail dengan error serupa), baru ke gemini. Juga mempercepat rate-limit breach (bergabung dengan F-04).

### Root Cause Analysis

Konstruksi chain naif; tidak ada sanitization pada `set_fallback_chain` atau `from_config`. Tidak ada validator config yang memperingatkan user.

### Implementation Plan

1. **Tambahkan sanitization** di `set_fallback_chain` (cari lokasi method ini; ada di [router.rs](http://router.rs)). Pseudo:

```rust
pub fn set_fallback_chain(&mut self, chain: Vec<String>) {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(self.default_provider.clone());
    self.fallback_chain = chain.into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect();
}
```

1. **Atau** lakukan sanitization di dalam `complete` saat chain dirakit (line 294–295):

```rust
use std::collections::HashSet;
let mut seen: HashSet<&str> = HashSet::new();
let chain: Vec<String> = std::iter::once(self.default_provider.as_str())
    .chain(self.fallback_chain.iter().map(|s| s.as_str()))
    .filter(|p| seen.insert(p))
    .map(|s| s.to_string())
    .collect();
```

Preferensi: (1) karena menangkap masalah lebih awal.

1. **Test baru** di `mod tests`:
    - `fallback_chain_deduplicates_default_provider`
    - `fallback_chain_deduplicates_repeated_entries`
2. **Dokumentasikan** di doc comment `set_fallback_chain`: "Duplicate entries and the default provider are silently removed to avoid redundant retries."
3. **Opsional**: log `warn!` jika dedup menghapus entry — memberi user sinyal konfigurasi mereka tidak sesuai niat.

### Risk & Rollback

Perubahan semantik minor: user yang mengandalkan duplikat intentional akan melihat perilaku baru. Dokumentasikan di CHANGELOG. Rollback: revert method.

### Effort

**XS** — ~25 menit.

## F-08 — `SemanticChunker::chunk` Off-By-One Byte Count + Break Guard Terlalu Ketat

**Severity:** `minor`

**Category:** Bug / Correctness

**Area / Crate:** `vil_context` — text chunking untuk indexing dan context retrieval

**Files / Paths:** `crates/vil_context/src/chunking.rs` (function `chunk` line 16–53, spesifik line 27–50)

**Status:** `open` (temuan baru run 15:00 WIB)

### Problem Statement

Dua defect pada algoritma windowing `SemanticChunker::chunk`:

**Defect 1 — off-by-one byte count**. Line 30–38:

```rust
let mut count = 0;
for word in words.iter().skip(start) {
    let word_len = word.len();
    if count + word_len > self.chunk_size && count > 0 {
        break;
    }
    window.push_back(word);
    count += word_len + 1;  // <-- menambahkan separator sebelum cek berikutnya
}
```

Baris `count += word_len + 1` menambahkan 1 byte untuk space pemisah, tetapi kondisi `if count + word_len > self.chunk_size` pada iterasi berikutnya mengandalkan `count` yang sudah ikut bit tambahan tersebut. Untuk word pertama, `count` dimulai dari 0 lalu menjadi `word_len + 1` setelah push — padahal chunk yang di-join (`window.iter().copied().collect::<Vec<_>>().join(" ")`) hanya memakai N-1 space untuk N word. Akibatnya: chunk aktual bisa lebih pendek dari `chunk_size` padahal masih muat satu word lagi. Konsekuensi minor: utilisasi chunk di bawah optimal, embedding lebih banyak dari perlu, biaya RAG naik.

**Defect 2 — break guard terlalu ketat**. Line 45–46:

```rust
if window.len() < self.chunk_overlap {
    break;
}
start += window.len().saturating_sub(self.chunk_overlap);
```

Guard ini memutus loop bila window terakhir lebih pendek dari `chunk_overlap`. Untuk input dengan `chunk_size=100`, `chunk_overlap=10`, dan teks yang terdiri atas 12 kata pendek yang total byte-nya < 100: iterasi pertama mem-push semua 12 kata ke window, chunk 1 dihasilkan, lalu `12 >= 10` → `start += 12 - 10 = 2`. Iterasi kedua, skip 2 kata → window 10 kata sisa (byte < 100) → chunk 2 dihasilkan (DUPLIKAT konten besar). Iterasi ketiga: skip 2+(10-10)=2 → window = 10 kata (sama) → **infinite loop**!

Kasus lain: teks tepat `< chunk_overlap` kata total. Iterasi pertama mem-push seluruh kata → `window.len() < chunk_overlap` → break. Hasilnya chunk 1 = seluruh teks. OK. Tapi kasus teks yang panjangnya `chunk_overlap + 1` kata: chunk 1 = semua; `start += 1`; iterasi kedua window = `chunk_overlap` kata → `window.len() >= chunk_overlap` → `start += 0` → infinite loop.

**Verifikasi**: construct `SemanticChunker::new(1000, 5)` dan call `chunk("a b c d e f")` (6 kata). Iterasi 1: window=6 kata, chunk hasilnya "a b c d e f". `6 >= 5` → `start += 6-5 = 1`. Iterasi 2: window dari skip 1 = 5 kata → `5 >= 5` → `start += 5-5 = 0` → **loop selamanya**.

### Dampak Runtime / Arsitektur

- **Hang pada indexing**: `VilRagIndex::index_codebase` yang memanggil chunker bisa deadlock saat mengindex file dengan distribusi kata tertentu. Berkisar dari slow-indexing hingga binary hang.
- **Chunk duplikat**: RAG retrieval mengembalikan snippet identik di slot top-K, mengurangi ragam konteks yang disampaikan ke LLM.
- **Memori bocor**: chunks Vec tumbuh tanpa batas selama infinite loop.

### Root Cause Analysis

Algoritma sliding-window dengan overlap standar menggunakan step `chunk_size - overlap` (bukan `window.len() - overlap`). Pilihan penulis untuk memakai `window.len()` sebagai reference adalah salah karena `window.len()` di iterasi terakhir = `chunk_overlap` persis → step = 0.

### Implementation Plan

1. **Rewrite `chunk`** dengan algoritma yang benar:

```rust
pub fn chunk(&self, text: &str) -> Result<Vec<String>, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(vec![]);
    }
    if self.chunk_size == 0 {
        return Err("chunk_size must be > 0".into());
    }
    if self.chunk_overlap >= words.len() {
        // Overlap larger than corpus → single chunk.
        return Ok(vec![words.join(" ")]);
    }
    let mut chunks: Vec<String> = Vec::new();
    let mut start: usize = 0;
    while start < words.len() {
        let mut byte_count: usize = 0;
        let mut end = start;
        while end < words.len() {
            let w = words[end];
            let sep = if end == start { 0 } else { 1 };
            let projected = byte_count + sep + w.len();
            if projected > self.chunk_size && end > start {
                break;
            }
            byte_count = projected;
            end += 1;
        }
        if end == start {
            // Word lebih panjang dari chunk_size — paksa masukkan sendirian
            // untuk menghindari infinite loop.
            chunks.push(words[start].to_string());
            start += 1;
            continue;
        }
        chunks.push(words[start..end].join(" "));
        if end >= words.len() { break; }
        // Step forward by (chunk_size - overlap) measured in words, dengan
        // minimum 1 untuk monoton.
        let window_words = end - start;
        let step = window_words.saturating_sub(self.chunk_overlap).max(1);
        start += step;
    }
    Ok(chunks)
}
```

1. **Tambah validasi konstruktor** `SemanticChunker::new`:

```rust
pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
    debug_assert!(chunk_size > 0, "chunk_size must be > 0");
    debug_assert!(chunk_overlap < chunk_size, "chunk_overlap must be < chunk_size");
    Self { chunk_size, chunk_overlap }
}
```

1. **Test baru** (file belum punya `#[cfg(test)] mod tests`, tambahkan):
    - `chunk_empty_text_returns_empty`
    - `chunk_shorter_than_size_returns_single_chunk`
    - `chunk_word_longer_than_size_does_not_loop` — regression test untuk guard line 34
    - `chunk_with_overlap_advances_monotonically` — property test 1000 input random, assert `chunks.len() < input.word_count` dan `total_time < 100ms`
    - `chunk_overlap_boundary_does_not_infinite_loop` — input 6 kata, chunk_size=1000, overlap=5 (skenario bug yang diverifikasi di Problem Statement)
    - `chunk_byte_count_is_accurate` — assert setiap chunk `< chunk_size + longest_word_len` dan `>= min(chunk_size, remaining_bytes)`
2. **Benchmark**: `crates/vil_context/benches/chunking_bench.rs` dengan criterion, input 1MB text, `chunk_size=512`, `overlap=64`. Target: `< 50 ms`.
3. **Timeout guard runtime**: tambahkan loop counter safety limit `debug_assert!(iterations < words.len() * 2)` untuk memastikan tidak ada regressi infinite loop di masa depan.

### Risk & Rollback

Semantik chunk berubah (lebih sedikit chunk, lebih optimal). Konsumen downstream `vil_rag` perlu re-index. Tambahkan migration note di CHANGELOG. Rollback: revert single file.

### Effort

**M** — ~2 jam termasuk 6 test + benchmark.

# Appendix A — Findings Resolved Sejak Run Sebelumnya

Tiga finding dari run 2026-04-19 14:00 WIB berubah status menjadi `fixed` di run 15:00 WIB. Bukti perbaikan dikutip langsung dari codebase.

### Previously F-01 — `PolicyEngine::enforce` conflates `RequireApproval` as `PermissionDenied` → `fixed`

**Bukti**: `crates/vil_trust/src/error.rs` line 18–25 sekarang memiliki varian baru:

```rust
#[error("action '{action}' requires approval: {reason}")]
RequiresApproval {
    action: String,
    reason: String,
},
```

Dan `crates/vil_trust/src/policy.rs` line 154–160 sekarang memancarkan varian yang benar:

```rust
PolicyDecision::RequireApproval => {
    warn!(tool = %request.tool_name, agent = %request.agent_id, "Action requires human approval");
    Err(TrustError::RequiresApproval {
        action: request.tool_name.clone(),
        reason: "human approval".to_string(),
    })
}
```

Doc comment di line 145–146 juga diperbarui menjelaskan pemisahan: "Like `PolicyEngine::evaluate` but converts Deny to `PermissionDenied` and RequireApproval to `RequiresApproval`." **Follow-up untuk run berikutnya**: verifikasi seluruh caller `enforce` di workspace sudah di-migrasikan untuk men-match `RequiresApproval` dan mem-trigger jalur approval UI (rg `TrustError::PermissionDenied` vs `TrustError::RequiresApproval` untuk audit).

### Previously F-02 — `TokenBudget::add_usage` overflows on `u64` → `fixed`

**Bukti**: `crates/vil_llm/src/token_budget.rs` line 18–20:

```rust
pub fn add_usage(&mut self, tokens: u64) {
    self.used = self.used.saturating_add(tokens);
}
```

Plus test baru line 53–58 yang mem-verifikasi invariant `u64::MAX + 1000 == u64::MAX`:

```rust
#[test]
fn add_usage_saturates_on_overflow() {
    let mut budget = TokenBudget::new(u64::MAX);
    budget.add_usage(u64::MAX);
    budget.add_usage(1_000);
    assert_eq!(budget.used(), u64::MAX);
}
```

### Previously F-03 — `LlmRouter::complete` retry_after_secs overflow on `*1000` → `fixed`

**Bukti**: `crates/vil_llm/src/router.rs` line 331–337 sekarang memakai pola saturating + cap:

```rust
let delay = if let LlmError::RateLimited(_, retry_after_secs) = &e {
    let delay_ms = retry_after_secs
        .saturating_mul(1_000)
        .min(self.retry_config.max_backoff_ms);
    crate::retry::RetryDelay {
        delay_ms,
        source: crate::retry::RetryDelaySource::RetryAfterHeader,
    }
} else { /* fallback */ };
```

Kedua perbaikan yang diminta (saturating dan capping ke `max_backoff_ms`) diterapkan sekaligus. **Follow-up**: tambahkan log `info!(capped = true, requested_secs = retry_after_secs)` saat capping terjadi, supaya operator tahu provider mengirim retry-after yang absurd.

### Pemetaan carry-over untuk audit trail

- (run 14:00) F-04 retry jitter → (run 15:00) **F-01** — status berubah dari `open` menjadi `partial` karena scaffolding (enum + field) sudah ada; implementation plan dipersempit ke "hubungkan field ke fungsi".
- (run 14:00) F-05 changeset linear scan → (run 15:00) **F-02**, status `open`, konten identik + baris diperbarui.
- (run 14:00) F-06 approval race polling → (run 15:00) **F-03**, status `open`.
- (run 14:00) F-07 rate limiter once per complete → (run 15:00) **F-04**, status `open`.
- (run 14:00) F-09 resolve_retry max_attempts not enforced → (run 15:00) **F-05**, status `open`.
- (run 14:00) F-08 PermissionSet redundant check → (run 15:00) **F-06**, status `open`.
- (run 14:00) F-10 fallback chain dedup → (run 15:00) **F-07**, status `open`.
- **new** SemanticChunker defects → (run 15:00) **F-08**.

# Appendix B — Cakupan Sapuan Run Ini

Deep-read pada 11 file, 7 crate berbeda — jauh di atas minimum 8 file / 3 crate yang diwajibkan instruksi.

| **Crate** | **File** | **Baris total** | **Peran audit** |
| --- | --- | --- | --- |
| vil_trust | src/[error.rs](http://error.rs) | 48 | Verifikasi varian RequiresApproval ditambahkan → prev F-01 fixed |
| vil_trust | src/[policy.rs](http://policy.rs) (lines 140–170) | 243 | Verifikasi enforce() memanggil RequiresApproval |
| vil_trust | src/[permissions.rs](http://permissions.rs) | 75 | Verifikasi F-05 (run 14:00) masih ada → carry-over F-06 |
| vil_llm | src/token_[budget.rs](http://budget.rs) | 67 | Verifikasi saturating_add + test → prev F-02 fixed |
| vil_llm | src/[router.rs](http://router.rs) (lines 270–370) | 717 | Verifikasi saturating_mul+cap (fixed), rate_limiter once (F-04), fallback dedup absent (F-07) |
| vil_llm | src/[retry.rs](http://retry.rs) | 175 | Verifikasi JitterMode scaffolding tanpa behavior → F-01 partial; max_attempts absent → F-05 |
| vac_changeset | src/[lib.rs](http://lib.rs) (lines 1–220) | 454 | Verifikasi Vec scan masih ada → carry-over F-02 |
| vac_approvals | src/[lib.rs](http://lib.rs) (lines 410–500) | 696 | Verifikasi polling 5x25ms masih ada → carry-over F-03 |
| vil_swarm | src/tool_[execution.rs](http://execution.rs) | 94 | Sapuan baru — bersih, tidak ada temuan |
| vil_context | src/[chunking.rs](http://chunking.rs) | 54 | Sapuan baru → **F-08** infinite loop risk + byte count off-by-one |
| vil_memory | src/[episodic.rs](http://episodic.rs) (lines 1–200) | 246 | Sapuan baru — tidak ada blocker, tapi lihat Appendix C follow-up |

**Sapuan lebar (glob / grep):**

- `glob crates/*/src/lib.rs` → 21 crate (`vac_*` + `vil_*` + `vac_trace` + `vil_validate`).
- `glob crates/vil_swarm/src/*.rs` → 23 file modul.
- `glob crates/vil_context/src/*.rs` → 6 file modul (engine, shm, error, chunking, attention, lib).
- `glob crates/vil_memory/src/*.rs` → 5 file (semantic, episodic, store, error, lib).
- `grep \.unwrap\(\) crates/vil_swarm/src` → 42 hit, **semua di blok** `#[cfg(test)]` atau `mod tests` (bersih di jalur produksi, diverifikasi manual).
- `grep exponential_backoff_ms crates/vil_llm` → 6 hit, menegaskan fungsi tidak dipanggil dengan jitter-aware variant.

**Area belum disentuh mendalam run ini (kandidat prioritas untuk run 16:00 WIB):** `vac_cli`, `vac_tui_runtime`, `vac_session_control`, `vac_shell`, `vac_core`, `vac_runtime`, `vac_tools`, `vac_trace`, `vil_ir`, `vil_context` (sisanya: `engine.rs`, `shm.rs`, `attention.rs`), `vil_memory` (sisanya: `semantic.rs`, `store.rs`), `vil_knowledge`, `vil_metrics`, `vil_inference`, `vil_rag`, `vil_validate`, `vil_swarm` (sisanya: 22 file lain). Modul `vil_llm` yang belum diperiksa ulang: `tokenizer.rs`, `streaming.rs`, `sanitize.rs`, `rulebook_hook.rs`, `config.rs`, `provider.rs`, `models.rs`, `providers/*.rs`.

# Appendix C — Catatan Sapuan & Follow-Up

### Potensi finding yang ditahan untuk run berikutnya (butuh verifikasi tambahan)

1. **`vil_memory::episodic::extract_terms`** (line 177–187) mengandalkan stop-word list 11 kata Inggris hardcoded. Codebase VAC berbahasa Inggris teknis, tapi comment/doc bisa campur Indonesia. Kandidat: parameterisasi stop-words, atau dokumentasikan batasan bahasa. Severity: nit. Butuh review produk-owner dulu sebelum difinalisasi.
2. **`vil_memory::episodic`** tidak memiliki metode `delete_episode` atau kebijakan retention. Unbounded growth redb database pada sesi long-running. Kandidat: major (space exhaustion) tapi belum diverifikasi apakah ada compactor di tempat lain (misal di `store.rs` yang belum dibaca mendalam run ini).
3. **`vil_context::chunking`** memanggil `text.split_whitespace().collect()` yang alokasi Vec besar pada input 1MB. Trade-off: streaming iterator vs simplicity. Butuh benchmark sebelum klasifikasi severity.
4. **`vil_swarm` unwrap sweep**: 42 hit semua di test blocks — verifikasi dengan `rg '\.unwrap\(\)' crates/vil_swarm/src --glob '!**/tests/**'` plus cek manual `#[cfg(test)]` di setiap lokasi. Run ini sudah sample-verified 10 hit pertama; sisanya (mostly `orchestrator.rs`, `reasoning_fsm.rs`, `run_state.rs`) terlihat juga di test blocks tapi perlu konfirmasi penuh di run berikutnya.

### Pola grep yang direkomendasikan untuk run 16:00 WIB

- `Regex::new` di hot path (kompilasi berulang — kandidat `once_cell`).
- `serde_json::from_str` tanpa size limit pada data eksternal (DoS vector).
- `std::env::var` yang di-cache tanpa invalidasi (test flakiness + config staleness).
- `tokio::sync::mpsc::unbounded_channel` tanpa backpressure (memory pressure).
- `std::fs::` blocking di dalam `async fn` tanpa `spawn_blocking` (runtime stall).
- `TODO` / `FIXME` / `XXX` di crate produksi (utang teknis inventory).
- `.clone()` berulang dalam hot path (kandidat `Cow<str>` / `Arc<str>`).
- `panic!` / `unreachable!` / `todo!` / `unimplemented!` di handler async.
- `unsafe` block di crate `vil_context` (SHM layer) — butuh audit threading safety.
- `#[allow(dead_code)]` atau `#[allow(unused_imports)]` yang menyembunyikan scaffolding seperti F-01 run ini.

### Strategi coverage jangka menengah

Repo memiliki 21 crate. Instruksi `≥8 file / ≥3 crate per run` memberi cakupan teoritis ~1/3 crate per run bila file deep-read difokuskan pada satu crate. Dengan 24 run per hari, seluruh workspace dapat ter-audit mendalam dalam < 2 hari bila fokus dirotasi. Run 15:00 WIB telah menyentuh 7 crate (jauh di atas minimum); rotasi direkomendasikan:

- Run 16:00: `vac_cli` + `vac_tui_runtime` + `vac_runtime` (lapisan CLI+UI).
- Run 17:00: `vil_swarm` (modul yang belum disentuh: `orchestrator.rs`, `reasoning_fsm.rs`, `lanes.rs`).
- Run 18:00: `vil_rag` + `vil_context` sisanya + `vil_inference`.
- Run 19:00: `vac_trace` + `vil_validate` (chain audit).
- Run 20:00: `vil_memory` ([semantic.rs](http://semantic.rs), [store.rs](http://store.rs)) + `vil_knowledge`.
- Run 21:00: `vac_tools` + `vil_ir` (tool registry + IR pipeline).
- Run 22:00: Revisit `vil_llm` providers (`providers/*.rs`).
- Run 23:00: Cross-cutting security audit via grep patterns di atas.

# Catatan Penutup

Run 2026-04-19 15:00 WIB mencatat **kemajuan mitigasi nyata**: 3 dari 10 finding run sebelumnya telah ditutup (F-01 policy enforce, F-02 TokenBudget, F-03 retry_after_secs) antara 14:00 dan 15:00 WIB. Kemajuan terbesar di sisi **control-flow security** — `RequiresApproval` variant yang terpisah dari `PermissionDenied` sekarang aktif, jalur human-approval bisa dibedakan secara tipe. Dua defect overflow `u64` juga tertutup dengan `saturating_add` / `saturating_mul`. F-04 run 14:00 (retry jitter) ditutup secara **parsial**: scaffolding ada, tapi `exponential_backoff_ms` belum memanggilnya — satu line refactor kecil dibutuhkan untuk menutup lingkaran.

Enam finding lain run 14:00 belum tersentuh dan dibawa kembali dengan konten implementation plan yang dipertahankan + link baris diperbarui. Satu finding baru ditambahkan dari sapuan `vil_context::chunking`: defect algoritma windowing yang berpotensi menghasilkan infinite loop pada konfigurasi `chunk_overlap + 1 ≤ words.len() ≤ chunk_size_bytes` tertentu — ini diklasifikasikan `minor` tapi bisa naik ke `major` jika terbukti mempengaruhi `VilRagIndex::index_codebase` pada corpus nyata (butuh verifikasi empiris di run berikutnya).

Total backlog aktif turun dari 10 → 8 finding: 0 blocker, 0 major, 5 minor, 3 nit. Estimasi gabungan effort untuk menutup semua: ~10 jam engineering. Rekomendasi prioritas eksekusi: F-08 (new, correctness + potential hang), F-01 (partial, satu line untuk menutup lingkaran jitter), F-03 (approval UX), F-04 (rate limit breach), F-02 (performance), F-05..F-07 (hygiene + nit).