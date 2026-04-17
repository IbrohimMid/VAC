# Tasks
- [x] Task 1: Audit Menyeluruh Codebase VAC Saat Ini
  - [x] SubTask 1.1: Menganalisis kapabilitas TUI dan UX (komponen visual, interaktivitas, state management di `crates/vac_cli`).
  - [x] SubTask 1.2: Menganalisis *core runtime*, *isolation*, dan arsitektur eksekusi *agentic* MCP/LLM (di `crates/vac_core`, `vac_tools`, `vil_llm`).
  - [x] SubTask 1.3: Menganalisis tingkat integrasi VIL (Rulebook, Score, Semantic Mode) dan status kesiapan produksi (seperti penanganan error, *buffer shell*, deteksi *secret*).
- [x] Task 2: Analisis Kompetitif (VAC vs Stakpak vs Claude Code)
  - [x] SubTask 2.1: Menentukan dimensi evaluasi (contoh: *Agentic Autonomy*, *Context Awareness*, *TUI Richness*, *Extensibility/MCP*, *Isolation/Security*).
  - [x] SubTask 2.2: Menilai profil kapabilitas Stakpak (berdasarkan *industry standard* untuk *AI-native DevOps/Shell*).
  - [x] SubTask 2.3: Menilai profil kapabilitas Claude Code (berdasarkan fitur *agentic CLI* terkini).
  - [x] SubTask 2.4: Memetakan VAC terhadap dimensi-dimensi tersebut untuk menemukan *gap* (kekurangan) dan *competitive advantage* (keunggulan).
- [x] Task 3: Penyusunan Dokumen Roadmap Strategis
  - [x] SubTask 3.1: Membuat dokumen `docs/COMPETITIVE_ANALYSIS.md` (atau mengintegrasikannya ke dalam laporan komprehensif) berisi temuan audit dan matriks perbandingan.
  - [x] SubTask 3.2: Membuat dokumen `docs/ROADMAP.md` dengan *milestone* taktis dan strategis yang jelas untuk mengejar ketertinggalan dan menyaingi kompetitor dalam fase Jangka Pendek, Menengah, dan Panjang.

# Task Dependencies
- [Task 2] bergantung pada pemahaman mendalam dari hasil audit di [Task 1].
- [Task 3] bergantung pada penyelesaian analisis dan pemetaan di [Task 1] dan [Task 2].