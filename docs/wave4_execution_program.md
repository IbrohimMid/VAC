# Program Eksekusi Wave 4: VAC Product Blueprint

## 1. Pendahuluan
Dokumen ini merupakan penjabaran strategis dan operasional dari blueprint Wave 4 (VAC Product Blueprint). Tujuannya adalah memecah blueprint menjadi matriks kapabilitas yang terukur, tahapan eksekusi yang berurutan (milestones), dan kriteria penerimaan (gates) yang sangat spesifik (PASS/PARTIAL/BLOCKER) untuk memastikan setiap fase dibangun dengan standar keamanan dan kualitas produksi (*production-grade*).

---

## 2. Capability Matrix

### 2.1 Target Parity Minimum
*Kapabilitas dasar yang wajib dimiliki agar VAC kompetitif dengan standar agen coding saat ini.*
* **Interactive Coding Loop**: Siklus otonom (observasi, perencanaan, modifikasi file, eksekusi alat) iteratif.
* **Tool Routing & Permissions**: Perutean pemanggilan alat dengan sistem izin (RBAC) terpusat.
* **Operator Approvals**: Alur persetujuan (*approval flow*) wajib untuk eksekusi berisiko tinggi.
* **Session Continuation**: Kemampuan menyimpan (*checkpoint*) dan memulihkan sesi (*resume*).
* **Repo-Aware Execution**: Pemahaman kontekstual terhadap struktur dan dependensi proyek.
* **Subagent Support**: Pendelegasian tugas ke sub-agen terspesialisasi.
* **Operator Visibility**: CLI/TUI dengan visibilitas *real-time* tentang pemikiran dan rencana agen.

### 2.2 Target Surpass (Diferensiasi Utama)
*Kapabilitas lanjutan yang membedakan VAC dari agen generik.*
* **VIL-Native Semantic Planning**: Perencanaan menggunakan representasi semantik (*Intermediate Representation/IR*).
* **IR-Aware Validation**: Validasi modifikasi kode secara struktural (memahami AST/IR).
* **Approval Persistence & Resume Safety**: Status persetujuan operator (*approved tools*) dipertahankan melintasi *checkpoint*.
* **Explicit Privacy Boundary**: Kontrak *privacy vault* seragam lintas Engine, Swarm, Subagent, dan Router.
* **Delegation Discipline**: Sub-agen beroperasi di *sandbox* ketat (*deny-by-default* untuk alat berisiko).

### 2.3 Intentional Differences
*Pilihan desain defensif yang mengutamakan keamanan dan auditabilitas.*
* **Runtime Approval Flow**: Persetujuan di tingkat *runtime* sebelum eksekusi berisiko.
* **Semantic Model Exposure**: Mengekspos jejak keputusan semantik untuk audit transparan.
* **Absolute Sandbox Policy**: Batas *sandbox* absolut yang ketat.

---

## 3. Milestone Eksekusi (Wave 4A - 4E)

### Wave 4A: Product Surface Consolidation
* **Fokus Utama**: Pemolesan CLI/TUI, aliran sesi, UX *diff* dan persetujuan, serta akurasi dokumentasi.
* **Target Output**: *Operator path* stabil, UX intuitif, dan dokumentasi jujur (sinkron dengan kode).

### Wave 4B: Trust & Safety Consolidation
* **Fokus Utama**: Propagasi *privacy vault*, persistensi *approval*, jalur *restore/substitute* di Router, dan *deny path* sub-agen di *sandbox*.
* **Target Output**: Arsitektur *secure-by-design*, kebal kebocoran privasi, dan bebas celah *bypass approval* saat *resume*.

### Wave 4C: Core Autonomy Maturity
* **Fokus Utama**: Pematangan *planning & validation loop*, disiplin pembatalan (*cancellation*), *resumability*, dan *auditability*.
* **Target Output**: Siklus otonomi agen stabil, tahan *crash*, dan dapat diaudit sepenuhnya.

### Wave 4D: Semantic Differentiation
* **Fokus Utama**: Kedalaman perencanaan VIL-native, validasi IR/semantik, integrasi *knowledge*, dan kepatuhan *Rulebook*.
* **Target Output**: Agen cerdas secara semantik dengan akurasi modifikasi struktural tinggi.

### Wave 4E: Product Hardening & Packaging
* **Fokus Utama**: Kesiapan produksi, mode *headless*/*autopilot*, alur instalasi, dan dokumentasi operasional.
* **Target Output**: Produk siap rilis (*Production-Ready*) untuk distribusi atau lingkungan CI/CD.

---

## 4. Acceptance Gates: PASS / PARTIAL / BLOCKER

### Gate 4A: Product Surface
* **PASS**: *Diff* jelas, *approval* responsif, status *real-time* akurat, README 100% sinkron.
* **PARTIAL**: *Lag* minor pada file sangat besar, fungsionalitas utama aman.
* **BLOCKER**: CLI *crash*, eksekusi tanpa konfirmasi di mode interaktif.

### Gate 4B: Trust & Safety
* **PASS**: Isolasi kredensial terbukti (*hard proof*), *approved_tools* persisten, sub-agen *sandbox* tertolak pada alat berisiko.
* **PARTIAL**: *(Tidak ada status PARTIAL untuk keamanan).*
* **BLOCKER**: Kredensial bocor, *sandbox bypass*, gagal memuat ulang persetujuan saat *resume*.

### Gate 4C: Core Autonomy
* **PASS**: Eksekusi multi-langkah berhasil, *graceful shutdown* pada pembatalan, *audit log* detail.
* **PARTIAL**: *Loop* redundan namun tugas selesai benar.
* **BLOCKER**: *Infinite loop*, *state* korup setelah batal, *validation* gagal deteksi *error syntax* dasar.

### Gate 4D: Semantic Differentiation
* **PASS**: Kepatuhan *Rulebook* terbukti, validasi IR menangkap *error* struktural, jejak rencana referensial akurat.
* **PARTIAL**: Pemahaman semantik optimal pada bahasa *tier-1*, *fallback* aman untuk bahasa minor.
* **BLOCKER**: *False-positive* validasi IR memblokir *commit*, agen mengabaikan *Rulebook*.

### Gate 4E: Product Hardening
* **PASS**: Mode *headless* stabil (gagal elegan jika butuh *approval* tanpa otorisasi), instalasi mulus, konfigurasi penuh.
* **PARTIAL**: Instalasi sukses namun butuh langkah manual minor.
* **BLOCKER**: Mode *headless* menggantung menunggu input, regresi fungsi inti, *memory leak*.

---

## 5. Execution Backlog per Wave

### P0 (Must-Have Before Claim) - Wave 4B & 4C
- Implementasi penuh *Privacy Vault* (Engine -> Swarm -> Subagent -> Router).
- Persistensi *approved_tools* di *checkpoint*.
- *Deny-by-default* untuk *sandboxed subagent*.
- *Graceful shutdown* dan *state preservation* saat pembatalan (Ctrl+C).

### P1 (Competitive Parity) - Wave 4A & 4E
- Refaktor TUI/CLI untuk *diff preview* yang informatif.
- Standarisasi alur *session continuation* (`vac resume`).
- Dukungan mode *headless* dan daemon *autopilot*.
- Skrip instalasi dan dokumentasi operasional (`onboarding.md`).

### P2 (Differentiation) - Wave 4D
- Integrasi *VIL-native semantic planning* menggantikan *text-based planning*.
- Implementasi *IR-Aware Validation* menggunakan `vil_validate`.
- Pemuatan dan penegakan *Rulebook* spesifik repositori.