# Final Code Review Follow-up Plan

**Status**: Active backlog  
**Last updated**: 2026-04-19  
**Scope**: `vil_llm`, `vac_tools`, `vil_swarm`, `vil_trust`, workspace CI/lint, and terminal cleanup behavior

## Purpose

Dokumen ini adalah plan implementasi final yang mengikuti audit code reviewer, tetapi sudah dinormalisasi agar akurat:

- item yang benar-benar kuat dianggap prioritas tinggi
- item yang cuma smell atau hardening tidak diperlakukan sebagai bug aktif
- tidak ada klaim produksi atau deployment yang dipalsukan
- plan ini terpisah dari [docs/tui_hardening_closure_plan.md](./tui_hardening_closure_plan.md); TUI closure sudah jadi backlog berbeda

## Audit Verdict Snapshot

| Item | Verdict | Priority | Decision |
| --- | --- | --- | --- |
| 1 | Partial | P2 | Harden invariant `SimpleRateLimiter` tanpa mengklaim race aktif |
| 2 | Accurate | P0 | Hentikan stub `anthropic-count-tokens` yang menyesatkan |
| 3 | Accurate as a fallback-quality issue | P2 | Pertahankan sebagai fallback saja, ukur akurasi, lalu kalibrasi bila perlu |
| 4 | Accurate | P1 | Pindahkan retryability ke status terstruktur |
| 5 | Partial | P2 | Hilangkan `Arc::get_mut().expect(...)` yang rapuh |
| 6 | Accurate | P1 | Hardening regex init dan compile coverage |
| 7 | Accurate as a design tradeoff | P2 | Tangani konsekuensi `panic = "abort"` pada terminal cleanup |
| 8 | Accurate | P2 | Tambah dependency-graph/layering gate |
| 9 | Accurate | P2 | Tambah lint gate untuk `unwrap` / `expect` production code |
| 10 | Accurate | P3 | Tambah docs crate-level untuk `vil_trust` |

## Workstream A: `vil_llm` correctness

### A1. Retry policy harus pakai status, bukan string match

**Masalah**

`is_retryable()` masih menebak retryability dari substring pesan provider. Ini fragile dan tidak konsisten lintas provider.

**Rencana**

- Tambahkan `status: Option<u16>` pada `LlmError::Provider`.
- Populasikan status dari provider HTTP layer yang memang punya status code.
- Ubah `is_retryable()` supaya memeriksa status terstruktur:
  - retryable: `429`, `502`, `503`, `504`
  - non-retryable: semua status lain kecuali ada alasan eksplisit lain

**Acceptance gate**

- Retryability tidak bergantung pada isi string pesan.
- Unit test menutup kasus `status = Some(503)` dan `status = Some(200)` dengan pesan yang menyesatkan.

### A2. `anthropic-count-tokens` tidak boleh tetap stub yang diam-diam fallback

**Masalah**

Feature flag `anthropic-count-tokens` sekarang memberi kesan ada token counting resmi, padahal implementasinya masih fallback lokal.

**Keputusan final**

Pilih salah satu jalur, dan **jangan dibiarkan ambiguitas**:

1. **Recommended path**: keluarkan stub dari production build dengan `compile_error!` sampai endpoint benar-benar diimplementasikan.
2. **Alternative path**: implementasikan HTTP call resmi ke `/v1/messages/count_tokens` dengan timeout, test server, dan fallback policy yang eksplisit.

**Acceptance gate**

- Tidak ada feature build yang mengaku memakai Anthropic count-tokens tetapi diam-diam menjalankan tokenizer lokal seolah-olah itu hasil resmi.
- README / docs mencerminkan perilaku yang sebenarnya.

### A3. `SimpleRateLimiter::acquire` diharden, bukan diklaim race bug

**Masalah**

`front().unwrap()` masih bergantung pada invariant implisit. Ini bukan race yang terbukti aktif saat ini, tetapi tetap rapuh.

**Rencana**

- Ganti `unwrap()` dengan branch aman:
  - jika deque kosong di saturated branch, return `None` atau hitung ulang state secara defensif
  - tambahkan `debug_assert!` untuk invariant yang memang diharapkan
- Tambahkan test untuk edge case prune-on-resume / prune-to-empty.

**Acceptance gate**

- Fungsi tidak panic pada state kosong setelah prune.
- Invariant dinyatakan eksplisit lewat assert atau branch defensif.

### A4. `with_rate_limit` tidak boleh bergantung pada `Arc::get_mut().expect(...)`

**Masalah**

Builder flow sekarang rapuh karena mengasumsikan `Arc` pasti belum di-clone.

**Rencana**

- Ubah setter rate limit menjadi rebuild/mutasi yang tidak tergantung pada `Arc::get_mut()`.
- Jika perlu, pecah builder terpisah agar kepemilikan eksklusif diekspresikan oleh tipe.
- Tambahkan test untuk call setelah clone / reuse builder.

**Acceptance gate**

- Tidak ada panic kalau router dipakai lewat builder flow yang sah.
- Rate-limit config tetap bisa diatur deterministik.

### A5. `HeuristicAdapter` tetap fallback, tetapi harus terukur

**Masalah**

Estimator `chars/4` terlalu kasar untuk kode dan non-ASCII.

**Rencana**

- Tetapkan `HeuristicAdapter` sebagai fallback only.
- Tambahkan corpus test kecil untuk mengukur error relatif terhadap tiktoken.
- Jika error melampaui threshold yang disepakati, baru pertimbangkan profile-based estimator.

**Acceptance gate**

- Dokumen dan nama adapter jujur tentang kualitas estimasi.
- Tidak ada klaim akurasi yang tidak dibuktikan dengan corpus test.

## Workstream B: security and operability

### B1. Regex redaction init harus fail-closed dan teruji

**Masalah**

`vac_tools::privacy` dan `vil_swarm::redaction` masih mengandalkan `Regex::new(...).unwrap()` di init global.

**Rencana**

- Centralize pattern list ke satu module per crate.
- Ganti panic message dengan error yang jelas dan tunggal.
- Tambahkan test yang memvalidasi seluruh pattern compile.
- Jika regex gagal compile, policy tetap fail-closed: jangan jalan dengan redaction yang setengah hidup.

**Acceptance gate**

- Regex pattern baru tidak bisa masuk tanpa test compile.
- Tidak ada startup failure yang misterius.

### B2. Consequence `panic = "abort"` harus ditangani secara eksplisit

**Masalah**

`panic = "abort"` di release profile berarti cleanup lewat `Drop` tidak dijamin jalan. Di repo ini, konsekuensi nyata yang terlihat adalah terminal cleanup pada `TerminalGuard`.

**Rencana**

- Tambahkan explicit terminal restore path di top-level error/exit handling.
- Audit `Drop` usage yang benar-benar penting dan pindahkan ke explicit shutdown bila ada path yang menyimpan state penting.
- Tambahkan catatan di runbook tentang konsekuensi panic/abort untuk operator.

**Acceptance gate**

- Panic path tidak meninggalkan terminal dalam state rusak.
- Tidak ada asumsi bahwa `Drop` selalu jalan pada release panic.

## Workstream C: guardrails and documentation

### C1. Tambah lint gate untuk `.unwrap()` / `.expect()` production code

**Masalah**

CI sudah menjalankan clippy, tetapi belum ada gate khusus untuk mencegah backslide penggunaan `.unwrap()` dan `.expect()` di production path.

**Rencana**

- Tambah lint policy bertahap:
  - tahap 1: warn untuk code produksi
  - tahap 2: deny setelah production surface bersih
- Allow test code tetap lebih longgar bila memang diperlukan.

**Acceptance gate**

- Production code tidak bisa menambah unwrap baru tanpa terlihat di CI.
- Test code tidak memblokir migrasi lint produksi.

### C2. Tambah dependency-graph/layering gate workspace

**Masalah**

Workspace sudah punya `cargo deny`, tetapi belum ada guard mekanis untuk arah dependency `vil_*` vs `vac_*`.

**Rencana**

- Tambah script layering check berbasis `cargo metadata`.
- Tambah CI job yang fail kalau edge melanggar aturan layering.
- Dokumentasikan target layering di docs arsitektur.

**Acceptance gate**

- Edge yang melanggar layer ketahuan sebelum merge.
- `cargo deny` tetap berjalan, layering check jadi guard tambahan.

### C3. Dokumentasi `vil_trust` harus menjelaskan model trust

**Masalah**

`vil_trust/src/lib.rs` terlalu tipis untuk crate keamanan.

**Rencana**

- Tambahkan crate-level docs:
  - trust zones
  - policy decisions
  - contoh alur `PolicyRequest`
  - link ke threat model
- Aktifkan `missing_docs` di crate ini.

**Acceptance gate**

- Crate security punya penjelasan model yang bisa dibaca tanpa menebak-nebak.

## Execution Order

### Urutan kerja yang disarankan

1. `A1` structured retryability
2. `A2` anthropic tokenizer stub decision
3. `A3` rate limiter invariant hardening
4. `A4` `with_rate_limit` builder cleanup
5. `B1` regex redaction hardening
6. `B2` panic/terminal cleanup
7. `C1` unwrap/expect lint gate
8. `C2` layering gate
9. `A5` heuristic adapter measurement
10. `C3` `vil_trust` docs

### Kenapa urutannya seperti itu

- langkah awal menutup correctness contract yang langsung memengaruhi runtime
- langkah tengah menutup robustness/security yang bisa panic atau leak
- langkah akhir menutup guardrail dan dokumentasi supaya tidak backslide

## Completion Definition

Plan ini boleh dianggap selesai kalau:

- retry policy tidak lagi berbasis string match
- feature `anthropic-count-tokens` tidak menyesatkan
- rate limiter dan builder tidak punya panic path rapuh
- regex redaction init punya test coverage
- terminal cleanup tidak bergantung pada asumsi `Drop` yang tidak dijamin
- lint gate produksi aktif
- layering gate aktif
- `vil_trust` terdokumentasi sebagai crate keamanan

## Non-goals

- Tidak ada klaim deployment production-ready hanya karena plan ini selesai.
- Tidak ada reopening TUI phase 1-12 di dokumen ini.
- Tidak ada penambahan fitur baru sebelum correctness dan guardrails di atas selesai.
