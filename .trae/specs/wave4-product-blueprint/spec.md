# WAVE 4 BLUEPRINT — VAC Product Blueprint Spec

## Why
VAC perlu bertransformasi dari sekadar codebase implementasi menjadi produk *agentic development* yang matang, dapat dioperasikan, dapat diaudit, dan layak bersaing dengan produk lain (seperti Claude Code atau Stakpak). Wave 4 bertujuan untuk mendefinisikan posisi VAC sebagai *VIL-native autonomous development engine*, membedakannya melalui kedisiplinan eksekusi semantik, batasan kepercayaan (*trust boundary*), dan *controlled autonomy*, bukan sekadar *prompt orchestration*.

## What Changes
- Menegaskan tujuan Wave 4: VAC berubah dari repo implementasi menjadi produk *agentic development* yang dapat dipakai operator nyata, dapat diaudit, dapat di-*resume*, dan layak dibandingkan secara serius.
- Mendefinisikan visi dan positioning: VAC sebagai **VIL-native autonomous development engine** (bukan clone Claude Code/Stakpak).
- Memformalkan persona dan non-target awal untuk memandu UX, runtime, dan safety trade-off.
- Menetapkan prinsip produk sebagai standar perilaku sistem (operator clarity, trust boundary eksplisit, artifacts-first, resume-safe autonomy, honest claims).
- Menetapkan ruang lingkup Wave 4 ke 5 domain produk (surface, runtime, trust/safety, multi-agent, repo intelligence).
- Memformalkan *capability blueprint* dan *architecture blueprint* sebagai kontrak lintas layer (engine, swarm, tools, surface).
- Memformalkan kontrak privasi dan persetujuan sebagai bagian dari runtime (*approval persistence*, *privacy vault propagation*, *approved replay*).
- Mendefinisikan acceptance criteria (produk/teknis/kompetitif) yang wajib bisa dibuktikan dengan tes/dokumen, bukan klaim.

## Impact
- Affected specs: Roadmap produk keseluruhan, standar penerimaan kualitas, dan fokus pengembangan fitur mendatang.
- Affected code: Akan memandu perubahan pada `vac_core`, `vil_swarm`, `vac_tools`, `vac_cli`, dan lapisan semantic intelligence (profil repo, rulebook, IR validator). Dokumentasi produk (README, arsitektur trust boundary, onboarding).

## ADDED Requirements
### Requirement: Product Surface Tiga Mode
Sistem HARUS menyediakan satu produk utuh dengan tiga mode operasi: interactive CLI/TUI, headless execution, dan autopilot/background mode, dengan perilaku yang konsisten terhadap trust boundary dan artifacts.

#### Scenario: Interactive Operator Mode
- **WHEN** Operator menjalankan VAC dalam mode interactive.
- **THEN** Sistem menyediakan input yang usable (termasuk multiline), status yang informatif, preview diff untuk perubahan, dan dialog approval yang jelas.

#### Scenario: Headless Execution
- **WHEN** Operator menjalankan objective secara headless (tanpa interaksi UI).
- **THEN** Sistem menjalankan task end-to-end, menghasilkan artifacts (diff/log/hasil validasi), dan gagal dengan error yang dapat ditindaklanjuti.

#### Scenario: Autopilot Background
- **WHEN** Operator menjalankan VAC dalam mode autopilot/background.
- **THEN** Sistem tetap mematuhi policy/approval yang sama dan menyediakan jejak audit untuk semua aksi.

### Requirement: VIL-Native Semantic Execution
Sistem HARUS mengutamakan eksekusi berbasis *typed Intermediate Representation* (IR) dan perencanaan semantik yang dapat diaudit, dibandingkan orkestrasi teks biasa.

#### Scenario: Subagent Delegation
- **WHEN** Agen mendelegasikan tugas ke subagen di dalam *sandbox*.
- **THEN** Subagen harus mewarisi *privacy vault* secara aman, beroperasi tanpa kebocoran variabel lingkungan induk, dan secara otomatis ditolak (Deny) jika mencoba mengakses alat yang memerlukan persetujuan (*approval*).

### Requirement: Operator Clarity Over Magic
Sistem HARUS memberi operator pemahaman yang jelas tentang apa yang sedang terjadi, kenapa terjadi, dan apa yang berubah, tanpa bergantung pada “magic behavior”.

#### Scenario: Diff Preview Sebelum Perubahan Repo
- **WHEN** Sistem akan melakukan perubahan file.
- **THEN** Operator dapat meninjau diff (atau ringkasan perubahan jika headless) yang cukup informatif untuk membuat keputusan.

#### Scenario: Failure Explanation
- **WHEN** Langkah eksekusi gagal (tool error, validasi gagal, policy deny).
- **THEN** Sistem mengembalikan penjelasan yang spesifik, mencantumkan langkah yang gagal, dan tindakan lanjutan yang masuk akal.

### Requirement: Trust Boundary Explicit By Design
Sistem HARUS memodelkan trust boundary sebagai bagian dari kontrak runtime: policy, sandbox, approval, privacy, dan auditability tidak boleh implisit.

#### Scenario: Tool Policy Enforcement
- **WHEN** Sebuah tool dieksekusi melalui router.
- **THEN** router menegakkan policy (allow/deny/needs approval) sebelum eksekusi, bukan setelahnya.

#### Scenario: Approved Replay After Resume
- **WHEN** Sesi di-*resume* dari checkpoint yang memiliki jejak approval sebelumnya.
- **THEN** tool yang telah disetujui dapat di-*replay* tanpa meminta approval ulang, namun tetap mengikuti policy dan boundary privasi yang sama.

### Requirement: Privacy Vault Propagation Contract
Sistem HARUS menjaga kontrak privasi berikut sebagai baseline:
- privacy vault utama berada di engine layer
- swarm dan subagent menerima referensi vault yang sama (bukan instance default baru)
- tool router melakukan restore secret sebelum execute, dan substitute setelah execute
- state approved dan privacy roundtrip bertahan pada checkpoint/resume

#### Scenario: Vault Shared Across Engine → Swarm → Subagent
- **WHEN** Engine menginisialisasi swarm dan swarm membuat subagent.
- **THEN** semua layer menggunakan vault yang sama untuk restore/substitute, tanpa duplikasi atau kebocoran.

#### Scenario: Sandbox Env Isolation
- **WHEN** Subagent berjalan dalam sandbox.
- **THEN** environment induk tidak bocor ke sandbox (kecuali yang eksplisit diizinkan policy), dan hasil tool tetap melalui substitute boundary.

### Requirement: Resume-Safe Autonomy
Sistem HARUS mampu menyimpan *state* yang kritis secara aman dan memulihkannya setelah jeda atau interupsi.

#### Scenario: Session Checkpoint and Resume
- **WHEN** Operator memulihkan sesi agen dari sebuah *checkpoint*.
- **THEN** Sistem harus mengembalikan semua daftar alat yang telah disetujui (`approved_tools`) dan *state* kritis lainnya secara persis seperti sebelum sesi dihentikan.

### Requirement: Persistence State Minimal
Sistem HARUS mem-*persist* informasi minimum berikut untuk mendukung auditability dan resume safety:
- stage / iterations
- total tokens (atau metrik biaya yang tersedia)
- pending approvals dan approved tools
- active tool calls (atau status eksekusi yang ekuivalen)
- ringkasan state yang dipangkas (*trim store/reduced context*) untuk resume
- session info penting untuk operator visibility

#### Scenario: Checkpoint Contains Approval-Critical State
- **WHEN** Sistem membuat checkpoint saat ada pending approval atau approved tools.
- **THEN** checkpoint memuat state tersebut sehingga resume tidak mengubah trust boundary.

### Requirement: Validation and Review Artifacts
Sistem HARUS menyediakan jalur validasi dan review yang menghasilkan artifacts yang dapat diperiksa: hasil validasi, file yang berubah/dibuat, dan status sukses/gagal yang jelas.

#### Scenario: Validation Pass After Changes
- **WHEN** Sistem selesai membuat perubahan pada repo.
- **THEN** sistem menjalankan validation pass yang relevan, melaporkan hasilnya, dan mencatat artifacts untuk audit/review.

### Requirement: Repo Intelligence Layer Baseline
Sistem HARUS mendukung baseline intelijen repo untuk meningkatkan kualitas perencanaan dan eksekusi: project profile, repo memory, semantic search, rulebook, knowledge injection, dan task history.

#### Scenario: Rulebook-Aware Execution
- **WHEN** Repo memiliki rulebook/aturan kontribusi.
- **THEN** perencanaan dan perubahan yang diusulkan harus mempertimbangkan rulebook tersebut sebelum eksekusi.

### Requirement: Honest Documentation
Dokumentasi produk HARUS sinkron dengan kemampuan aktual. Klaim yang tidak punya bukti implementasi (tes/artifact) dianggap tidak valid untuk Wave 4.

#### Scenario: Release Claim Requires Proof
- **WHEN** README atau dokumen arsitektur menyatakan sebuah kemampuan (mis. approval persistence).
- **THEN** kemampuan tersebut memiliki referensi ke bukti (tes, smoke test, atau demonstrasi artifact) yang dapat dijalankan atau ditinjau.

## MODIFIED Requirements
### Requirement: Operator Approval Flow
Alur persetujuan (*approval flow*) HARUS menjadi kontrak *runtime* yang solid, bukan sekadar elemen antarmuka pengguna (UI). Status persetujuan harus tercatat pada *checkpoint metadata*, dapat diaudit, dapat di-*replay* secara sah setelah resume, dan mematuhi batas *sandbox* (subagent tidak boleh bypass approval path).

## Acceptance Criteria
### Product Acceptance
- VAC dapat dijelaskan sebagai produk utuh dalam satu narasi yang konsisten (surface → engine → swarm → tools → intelligence).
- Operator dapat menjalankan workflow development nyata tanpa kebingungan mendasar (input, diff, approval, status, error).
- Approval flow tidak ambigu dan tidak mudah dibypass.
- Perubahan repo dapat ditinjau dan direproduksi (diff + artifacts).
- Sesi dapat di-*resume* tanpa kehilangan state kritis (terutama approval/privacy).
- Trust boundary tidak bocor dan failure mode dapat dijelaskan.

### Technical Acceptance
- Tersedia test coverage untuk jalur kritis (golden path) dan kontrak trust.
- Privacy vault propagation terbukti pada engine, swarm, subagent, dan tool router.
- Approval persistence terbukti setelah checkpoint roundtrip (termasuk approved replay).
- Deny path untuk sandboxed subagent terbukti.
- `route` dan `route_approved` sama-sama menjaga privacy roundtrip (restore/substitute) dan auditability.

### Competitive Acceptance
- VAC tidak tampak sebagai repo eksperimen; produk story dapat dipertanggungjawabkan.
- VAC punya alasan kuat untuk dipilih selain “AI coding agent lain”: VIL-native planning + typed execution discipline + controlled autonomy.

## Capability Matrix Target (Wave 4)
### Target Parity Minimum
- Interactive coding loop end-to-end (repo read → plan → edit → run tools → iterasi).
- Tool routing & permissions (policy enforce) + approval flow.
- Session continuation (checkpoint/resume) untuk pekerjaan panjang.
- Repo-aware execution (minimal: discovery/struktur repo + pencarian konteks).
- Subagent support (delegation) dengan sandbox dan policy.
- Operator visibility (status, diff, validation output, error explanation).

### Target Surpass (Diferensiasi)
- VIL-native semantic planning dan decomposition yang tidak “prompt chaining” generik.
- IR-aware validation (semantic/typed) sebagai bagian dari loop, bukan opsional.
- Approval persistence + resume safety sebagai kontrak runtime yang teruji.
- Privacy boundary eksplisit dan terdokumentasi lintas layer (vault shared, restore/substitute).
- Delegation discipline (subagent kuat tapi tetap bounded).

### Intentional Differences
- Gaya UX/command surface boleh berbeda selama operator clarity dan trust model tetap kuat.
- Tingkat keketatan policy enforcement boleh lebih tinggi dari baseline kompetitor (kontrol > “magic”).
- Cara semantic model diekspos ke operator boleh berbeda (ringkasan IR, explainability, rulebook).

## Milestone Eksekusi Wave 4 (4A–4E) dan Gate
### Definisi Status Gate
- **PASS**: Semua bukti wajib (hard proof + smoke) tersedia dan lulus; tidak ada blocker pada security/data/trust.
- **PARTIAL**: Ada gap non-kritis dengan mitigasi jelas, owner, dan bukti rencana re-test; tidak menyentuh security/data/trust boundary.
- **BLOCKER**: Ada pelanggaran trust boundary, kebocoran secret, kehilangan state approval/privacy, korupsi state checkpoint, atau ketidakmampuan operator meninjau perubahan.

### 4A — Product Surface Consolidation
- Deliverables:
  - Operator path stabil: input (termasuk multiline), status, diff preview, approval dialog yang jelas.
  - Session lifecycle UX: start/restore, navigasi error, dan recovery yang tidak menyesatkan.
  - README/Docs “honest claims” baseline.
- Gate PASS bukti minimum:
  - Smoke: jalur interactive “plan → diff → approval → apply” berjalan dan output dapat dipahami operator.
  - Hard proof: test/fixture untuk render approval + diff preview + session restore happy path.

### 4B — Trust & Safety Consolidation
- Deliverables:
  - Privacy vault propagation menyatu lintas layer (engine/swarm/subagent/router).
  - Router restore/substitute + approved replay path.
  - Sandboxed subagent deny path untuk tool yang butuh approval.
- Gate PASS bukti minimum:
  - Hard proof: tes privacy propagation (engine→swarm→subagent), tes `approved_tools` persistence setelah checkpoint, tes deny path subagent.
  - Hard proof: tes route privacy roundtrip dan route_approved privacy roundtrip.
  - Smoke: skenario end-to-end yang memicu restore/substitute dan menunjukkan tidak ada secret leak pada output operator.

### 4C — Core Autonomy Maturity
- Deliverables:
  - Planning loop + review/validation loop resilient (error handling jelas, audit trail cukup).
  - Cancellation discipline (safe cancel) dan resumability (checkpoint konsisten).
  - Run state cukup kaya untuk audit dan review.
- Gate PASS bukti minimum:
  - Hard proof: tes cancellation → checkpoint → resume tanpa kehilangan approval-critical state.
  - Smoke: long-running objective yang dapat dibatalkan dan dilanjutkan tanpa korupsi state.

### 4D — Semantic Differentiation
- Deliverables:
  - VIL-native planning depth (decomposition yang domain-aware dan konsisten dengan model semantik).
  - IR/semantic validation terintegrasi ke loop.
  - Repo intelligence (project profile, rulebook, memory) aktif mempengaruhi eksekusi.
- Gate PASS bukti minimum:
  - Hard proof: tes/fixture IR-aware validation pada perubahan/refactor representative.
  - Smoke: rulebook-aware execution (rule diterapkan) dan output menjelaskan alasan/constraint.

### 4E — Product Hardening & Packaging
- Deliverables:
  - Configuration story (mis. `vac.toml`) jelas dan robust.
  - Headless mode polish dan autopilot mode dengan policy/approval konsisten.
  - Onboarding dan operational docs siap dipakai operator.
- Gate PASS bukti minimum:
  - Smoke: headless run menghasilkan artifacts dan exit status yang jelas.
  - Hard proof: smoke coverage integrasi antar layer + dokumentasi instalasi/konfigurasi yang dapat diikuti.

## Mapping Gate → Bukti (Hard Proof / Smoke / Artifacts)
### Bukti wajib lintas milestone
- Hard proof tests:
  - Privacy vault propagation (engine→swarm→subagent)
  - `approved_tools` + pending approvals persistence setelah checkpoint roundtrip
  - Deny path sandboxed subagent pada tool “needs approval”
  - Router restore/substitute roundtrip (route dan route_approved)
- Smoke tests:
  - Interactive golden path (input → plan → diff → approval → apply → validation → summary)
  - Headless golden path (objective → execute → validation → artifacts)
- Artifacts:
  - Diff yang dapat ditinjau, ringkasan validasi, dan jejak audit (decision/tool history) untuk tiap run
  - Dokumen trust boundary (privacy + approval contracts) yang mengacu ke bukti uji

## Backlog Prioritas (Baseline)
### Must-have before claim
- Trust boundary proofs (privacy propagation, approval persistence, deny path, restore/substitute).
- Operator usability proofs (approval clarity, diff, error explanation, session restore UX).
- Resumability & cancellation proofs untuk long-running execution.

### Competitive parity
- Loop coding end-to-end yang stabil (edit/run/test/iterate) dengan repo awareness.
- Multi-agent delegation yang predictable dan bounded.
- Validasi/review artifacts yang bisa di-audit.

### Differentiation
- VIL-native planning yang dapat ditunjukkan lewat artifacts (IR summary/explainability).
- IR-aware validation sebagai default posture.
- Rulebook/knowledge injection yang nyata mempengaruhi keputusan eksekusi.

## REMOVED Requirements
Tidak ada requirement yang dihapus pada fase spesifikasi ini; Wave 4 bersifat konsolidasi dan pematangan kontrak produk.
