# Strategic Roadmap: Elevating VAC to World-Class Agentic CLI

Berdasarkan hasil audit komprehensif dan analisis kompetitif melawan **Stakpak** dan **Claude Code**, berikut adalah peta jalan (roadmap) strategis yang dirancang untuk mengatasi kesenjangan (gap) dan memaksimalkan keunggulan kompetitif (competitive advantage) VAC dalam hal TUI, UX, otonomi, dan integrasi VIL.

## Visi Jangka Panjang
Menjadikan VAC sebagai antarmuka terminal (TUI) *agentic* paling *observability-driven* di dunia, yang memadukan eksekusi otonom tingkat tinggi dengan kepatuhan tata kelola proyek berbasis VIL (Semantic Mode & Rulebook).

---

## Fase 1: Jangka Pendek (Menutup Gap Dasar & UX Polish)
**Fokus Utama:** Kestabilan, *Context Awareness* Otomatis, dan Pengalaman Pengguna (UX)

- [ ] **1. Auto-Discovery Context Engine**
  - **Goal:** Membuat VAC sadar akan *git status*, direktori aktif, *linter errors*, dan file yang sedang diedit (mirip Claude Code).
  - **Tindakan:** Mengembangkan modul `ContextCrawler` di `vac_core` untuk otomatis *inject* metadata proyek ke *prompt* sistem.
- [ ] **2. Peningkatan "Ask-User" UX**
  - **Goal:** Mengurangi friksi interaksi antara *operator* dan *agent*.
  - **Tindakan:** Menyempurnakan desain komponen `AskUser` di TUI, mendukung *fuzzy search* pada *multi-select*, serta *keyboard shortcuts* untuk *fast-approval*.
- [ ] **3. Hardening Kesiapan Produksi**
  - **Goal:** Stabilitas *runtime*.
  - **Tindakan:** Optimalisasi lebih lanjut *circular buffer* pada shell output (mengurangi overhead memori), penguatan deteksi *secret* menggunakan model probabilistik ringan, dan penguatan penanganan *error* TLS/MCP agar tidak *silent fail*.

---

## Fase 2: Jangka Menengah (Agentic Autonomy & Self-Healing)
**Fokus Utama:** Kemampuan Bernalar, Otonomi, dan Ekstensi MCP

- [ ] **1. Agentic Reasoning Loop (Self-Healing)**
  - **Goal:** Memungkinkan VAC mencoba perintah, mendeteksi *error*, menganalisis penyebab, dan memperbaikinya secara otonom sebelum meminta bantuan operator (seperti Claude Code).
  - **Tindakan:** Membangun *state machine* `VilReasoningLoop` di `vil_llm` yang menampung *chain-of-thought* LLM.
- [ ] **2. TUI "Workstation" VIL Tingkat Lanjut**
  - **Goal:** Memaksimalkan keunggulan kompetitif VAC di *observability*.
  - **Tindakan:** Menambahkan visualisasi *graph* ketergantungan *Rulebook*, *Semantic Score timeline* dalam bentuk *chart* teks ASCII, dan panel *log streaming* *real-time* di tab `WorkbenchTab::Vil`.
- [ ] **3. Ekosistem Plugin MCP Bawaan**
  - **Goal:** Memperluas kapabilitas integrasi pihak ketiga (menyaingi Stakpak).
  - **Tindakan:** Menyediakan *registry* MCP bawaan untuk *cloud providers* (AWS, GCP), sistem CI/CD, dan *ticketing* (Jira, GitHub).

---

## Fase 3: Jangka Panjang (Governance & Multi-Agent Orchestration)
**Fokus Utama:** Kolaborasi Skala Enterprise dan Orkestrasi Ekosistem

- [ ] **1. Multi-Agent Workflows**
  - **Goal:** Menjalankan beberapa agen secara paralel untuk menangani tugas berbeda (contoh: *QA Agent* dan *Dev Agent*) di dalam satu sesi VAC.
  - **Tindakan:** Membangun *scheduler* orkestrasi *multi-agent* pada `vac_core` dengan visibilitas antrean tugas di TUI.
- [ ] **2. VIL Governance Enforcement**
  - **Goal:** Menjadikan *Rulebook* dan *Semantic Mode* sebagai gerbang eksekusi *mandatory*.
  - **Tindakan:** Memblokir *merge* atau eksekusi *deployment* apabila *VIL Score* di bawah ambang batas yang ditentukan. TUI akan menampilkan *Banner* kritis jika *governance* dilanggar.
- [ ] **3. Cloud-Sync & Team Collaboration**
  - **Goal:** Berbagi sesi eksekusi *agentic* antar anggota tim.
  - **Tindakan:** Membangun layanan *backend* opsional untuk menyinkronkan *transcript* VAC, status TUI, dan *approval queue* secara *real-time*.

---

## Kesimpulan
Melalui *roadmap* tiga fase ini, VAC tidak hanya mengejar fitur-fitur otonomi *cutting-edge* yang ditawarkan Claude Code dan Stakpak, namun juga memperkuat keunikan utamanya: **Visibilitas Terminal yang Kaya (TUI)** dan **Tata Kelola Proyek Berbasis VIL**. Implementasi *roadmap* ini akan memastikan VAC tetap relevan dan dominan di ekosistem *AI-native developer tools*.