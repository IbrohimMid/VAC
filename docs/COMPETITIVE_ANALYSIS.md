# Competitive Analysis: VAC vs Stakpak vs Claude Code

## 1. Executive Summary
Dokumen ini menyajikan hasil audit komprehensif terhadap kapabilitas *Virtual Agentic CLI* (VAC) pada saat ini, dan membandingkannya dengan *industry leaders* di ranah *Agentic DevOps/CLI* yaitu **Stakpak** dan **Claude Code**. Tujuan analisis ini adalah untuk mengidentifikasi kekuatan, kelemahan, dan peluang strategis guna menjadikan VAC sebagai alat *world-class*.

## 2. Audit Codebase VAC Saat Ini

### 2.1 Arsitektur & Core Runtime (`vac_core`, `vac_tools`, `vil_llm`)
- **Isolation & Security:** VAC memiliki isolasi proses (PTY) dan deteksi *secret* tingkat dasar-menengah (mendeteksi URL dan *Generic/Bearer token*). Buffer *shell* sudah dibatasi menggunakan *circular buffer* untuk mencegah *Out-of-Memory*.
- **Integrasi LLM:** Modul `vil_llm` telah mendukung pemanggilan model bahasa dengan mekanisme *Token Bucket Rate Limiting* guna menjaga stabilitas penggunaan *cost*.
- **Extensibility:** VAC telah mengadopsi standar **MCP (Model Context Protocol)** untuk memperluas kapabilitasnya, dengan kemampuan mendeteksi status TLS dan meneruskan error ke operator.

### 2.2 TUI dan User Experience (`vac_cli`)
- **Visual & Layout:** Menggunakan arsitektur terminal modern dengan berbagai panel (*side panel*, *workbench tab*, *banner queue*). Terdapat *Workstation VIL* tersendiri untuk menampilkan metrik spesifik proyek (*Score*, *Rulebook*, *Semantic Mode*).
- **Interaktivitas:** Memiliki mekanisme `Ask-User` (*single/multi-select*, *free text*) yang kuat untuk meminta konfirmasi (Human-in-the-loop/HITL). Terdapat sistem *prompt detection* untuk berbagai tipe *shell* (Unix/Windows/SQL).
- **Notifikasi:** `Banner Queue` berbasis tingkat keparahan (*severity*) untuk menyajikan *alert* dan status penting kepada pengguna.

---

## 3. Matriks Perbandingan Kompetitif

Berikut adalah perbandingan *head-to-head* antara **VAC**, **Stakpak**, dan **Claude Code** berdasarkan beberapa dimensi krusial.

| Dimensi Evaluasi | VAC (Saat Ini) | Stakpak | Claude Code |
| :--- | :--- | :--- | :--- |
| **Agentic Autonomy** | Menengah. Sangat bergantung pada struktur VIL dan konfirmasi HITL (Ask-User). | Tinggi. Otonomi DevOps untuk *infrastructure-as-code* dan *cloud deployment*. | Sangat Tinggi. Autonomi berbasis penalaran (*chain-of-thought*) untuk *coding* & *execution* langsung. |
| **Context Awareness** | Terbatas pada direktori aktif, *Rulebook*, dan metadata VIL. | Berfokus pada konteks arsitektur *cloud* dan *DevOps state*. | Luas. *Auto-discovery* file, riwayat *git*, dan linting secara otomatis. |
| **TUI Richness & UX** | Kuat. *Banner queue*, *dedicated workstation tab*, multi-panel layout. | Fungsional, berpusat pada status *deployment* dan metrik infrastruktur. | Minimalis namun sangat intuitif (*streamlined chat-like interface* di terminal). |
| **Extensibility** | MCP Client (Standar industri yang *future-proof*). | Plugin spesifik untuk *cloud providers* (AWS, GCP, Terraform). | Standar internal Anthropic dengan eksekusi *bash* otonom. |
| **Security & Trust** | Deteksi *secret* heuristik, eksekusi PTY terisolasi. | Role-Based Access Control (RBAC), *cloud credential vault*. | *Sandboxed execution*, persetujuan (*approval*) untuk perintah destruktif. |

---

## 4. Analisis Gap & Keunggulan Kompetitif

### 4.1 Gap (Kekurangan VAC)
1. **Kurangnya Context Auto-Discovery:** Claude Code mampu menelusuri repositori, menemukan pesan *error* di *linter*, dan membaca *git history* secara otomatis tanpa diinstruksikan. VAC masih memerlukan *rulebook* eksplisit.
2. **Keterbatasan Otonomi Penalaran:** VAC belum memiliki *agentic reasoning loop* yang bisa mencoba, gagal, dan memperbaiki sendiri (*self-healing*) sehalus Claude Code.
3. **Integrasi Cloud/DevOps:** Tidak seperti Stakpak yang *native* mengerti *infrastructure*, VAC masih bersifat *general-purpose shell*.

### 4.2 Competitive Advantage (Kekuatan VAC)
1. **Rich TUI & Operator Observability:** Tidak ada kompetitor yang menyajikan *dashboard terminal* selengkap VAC (*Banner Queue*, *VIL Workstation*, *Side Panels*). Operator VAC memegang kontrol penuh secara visual.
2. **MCP-First Architecture:** Dengan mengadopsi Model Context Protocol (MCP) sejak awal, VAC memiliki potensi ekstensi tanpa batas ke berbagai *tool* eksternal tanpa harus *hardcode* integrasinya.
3. **VIL Semantic Alignment:** Kemampuan VAC mengukur tingkat kepatuhan kode terhadap "Rulebook" (VIL Score) adalah fitur unik yang menjembatani *governance* dan eksekusi.