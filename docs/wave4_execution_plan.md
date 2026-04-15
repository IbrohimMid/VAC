# Wave 4 Execution Plan: VAC Product Blueprint

## 1. Capability Matrix

### 1.1 Target Parity Minimum
- **Interactive Coding Loop**: Agen dapat merencanakan, mengedit file, dan menjalankan alat dalam iterasi berkelanjutan.
- **Tool Routing & Permissions**: Perutean alat yang andal dengan izin akses yang dikendalikan.
- **Approvals**: Alur persetujuan operator untuk alat berisiko tinggi.
- **Session Continuation**: Kemampuan untuk menyimpan dan memulihkan *state* sesi.
- **Repo-Aware Execution**: Pemahaman dasar tentang struktur dan konten repositori.
- **Subagent Support**: Kemampuan mendelegasikan tugas ke subagen.
- **Operator Visibility**: Tampilan yang jelas tentang apa yang sedang dilakukan agen.

### 1.2 Target Surpass (Diferensiasi Utama)
- **VIL-Native Semantic Planning**: Perencanaan berbasis *Intermediate Representation* (IR), bukan sekadar teks.
- **IR-Aware Validation**: Validasi perubahan kode menggunakan pemahaman semantik VIL.
- **Approval Persistence & Resume Safety**: Status persetujuan (*approved tools*) bertahan melintasi *checkpoint* dan *restart* dengan aman.
- **Explicit Privacy Boundary**: Kontrak *privacy vault* yang seragam dari Engine, Swarm, hingga Subagent dan Router.
- **Delegation Discipline**: Subagen berjalan dalam *sandbox* yang ketat, tanpa kebocoran lingkungan (*environment*), dan *deny-by-default* untuk alat yang butuh *approval*.

### 1.3 Intentional Differences
- **Gaya UX**: Pendekatan interaktif yang terintegrasi erat dengan *approval flow* di tingkat *runtime*.
- **Semantic Model Exposure**: Operator dapat melihat jejak keputusan semantik agen.
- **Policy Enforcement**: Jauh lebih ketat dibandingkan *coding agent* umum (mis. batas *sandbox* absolut).

---

## 2. Execution Milestones (4A - 4E)

### Wave 4A: Product Surface Consolidation
- **Fokus**: Pemolesan CLI/TUI, *session flow*, UX untuk *diff/approval*, kejelasan status, dan kejujuran README.
- **Output**: *Operator path* stabil, dokumentasi sinkron, dan *product surface* saat ini jelas.

### Wave 4B: Trust & Safety Consolidation
- **Fokus**: Propagasi *privacy vault*, persistensi *approval* di *checkpoint*, jalur *restore/substitute* di *router*, penolakan (*deny path*) untuk subagen di *sandbox*, serta *smoke tests*.
- **Output**: Kontrak privasi menyatu di semua lapisan, dan alur *approval* aman saat di-*resume*.

### Wave 4C: Core Autonomy Maturity
- **Fokus**: *Planning loop*, *validation loop*, disiplin pembatalan (*cancellation*), *resumability*, dan *auditability*.
- **Output**: *Agent loop* tidak hanya berjalan, tetapi dapat dipercaya dan diaudit.

### Wave 4D: Semantic Differentiation
- **Fokus**: Kedalaman perencanaan VIL-native, validasi IR/semantik, integrasi *knowledge* dan profil repositori, serta eksekusi yang sadar *rulebook*.
- **Output**: VAC memiliki diferensiasi kuat yang sulit ditiru oleh agen generik.

### Wave 4E: Product Hardening & Packaging
- **Fokus**: Pemolesan konfigurasi, mode *headless*, mode *autopilot*, alur instalasi/*onboarding*, dan dokumentasi operasional.
- **Output**: VAC layak dipresentasikan dan digunakan sebagai produk serius.

---

## 3. Acceptance Gates & Proof Mappings

Setiap milestone harus melewati *gate* penerimaan dengan kriteria berikut:

### Gate 4A: Product Surface
- **Kriteria (PASS)**: Operator dapat melihat *diff*, menyetujui tugas, dan membaca status tanpa kebingungan; README akurat.
- **Proof**: Pengujian manual alur UX, *smoke test* TUI *rendering*, dan ulasan dokumen README.

### Gate 4B: Trust & Safety
- **Kriteria (PASS)**: *Privacy vault* digunakan seragam; *approval* tersimpan di *checkpoint*; subagen ditolak saat mencoba alat berbahaya.
- **Proof**: 
  - *Hard proof test*: `run_state` (persistensi *approval*).
  - *Hard proof test*: `subagent` (pembagian *vault* & batas *sandbox*).
  - *Hard proof test*: `router` (jalur *deny* & *restore/substitute*).
  - *Smoke test*: Injeksi *vault* dari `engine` ke `swarm`.

### Gate 4C: Core Autonomy
- **Kriteria (PASS)**: Agen dapat merencanakan, memvalidasi, dan dihentikan secara aman tanpa merusak *state*.
- **Proof**: *Integration tests* untuk *planning* dan *validation loops*; *unit tests* untuk *cancellation* dan *state integrity*.

### Gate 4D: Semantic Differentiation
- **Kriteria (PASS)**: Perencanaan dan eksekusi menggunakan IR dan *rulebook* secara efektif.
- **Proof**: *Golden flow tests* untuk refaktor berbasis IR dan kepatuhan terhadap *rulebook*.

### Gate 4E: Product Hardening
- **Kriteria (PASS)**: Konfigurasi stabil, instalasi lancar, dan tidak ada regresi di mode *headless/autopilot*.
- **Proof**: *End-to-end (E2E) smoke tests* untuk proses *onboarding* dan mode eksekusi penuh.

---

## 4. Prioritized Backlog

### Must-Have Before Claim (P0)
*Fitur yang wajib ada sebelum VAC dapat diklaim sebagai produk yang aman dan terpusat.*
- Implementasi penuh *Privacy Vault* lintas batas (Engine -> Swarm -> Subagent -> Router).
- Persistensi *approved_tools* di *checkpoint*.
- *Deny-by-default* untuk *sandboxed subagent* pada alat yang butuh *approval*.
- Dokumentasi arsitektur privasi yang jujur dan akurat.

### Competitive Parity (P1)
*Fitur untuk menyamai standar industri (mis. Claude Code, Stakpak).*
- TUI/CLI yang responsif dengan *diff preview* yang jelas.
- *Session continuation* yang andal.
- Kemampuan dasar agen untuk membaca, merencanakan, dan mengedit file.

### Differentiation (P2)
*Fitur yang membuat VAC unik.*
- Perencanaan dan eksekusi berbasis VIL IR.
- *Validation loop* yang sadar semantik.
- Integrasi *Rulebook* dan profil repositori ke dalam alur kerja agen.