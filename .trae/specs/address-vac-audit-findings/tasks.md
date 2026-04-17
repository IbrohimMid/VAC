# Tasks
- [ ] Task 1: Rekonsiliasi State Ask-User (Unit 7)
  - [ ] SubTask 1.1: Periksa `types.rs` dan file state terkait untuk memastikan `ask_user_question_kind`, `ask_user_multi_selected`, dan `ask_user_metadata` terdefinisi.
  - [ ] SubTask 1.2: Perbarui model state agar sesuai dengan yang digunakan oleh `ask_user.rs` dan memastikan data metadata dapat melalui *round-trip*.
- [ ] Task 2: Implementasi VIL Workstation Penuh (Unit 9)
  - [ ] SubTask 2.1: Tambahkan `WorkbenchTab::Vil` pada definisi tab TUI (`AppState`).
  - [ ] SubTask 2.2: Pindahkan komponen terkait VIL (`VilStatusSnapshot`, dll.) ke dalam tab dedicated ini agar menjadi workstation VIL terpisah.
- [ ] Task 3: Perbaikan Isu Kritis Kesiapan Produksi (Shell, Security, Rate Limit, MCP)
  - [ ] SubTask 3.1: Implementasikan batas ukuran (bound) pada buffer output shell, misalnya dengan *circular buffer* agar tidak *unbounded*.
  - [ ] SubTask 3.2: Perbarui logika deteksi secret (sebelumnya terlalu sederhana/naif) menjadi lebih tangguh (contoh: *regex pattern* yang lebih komprehensif atau *entropy check*).
  - [ ] SubTask 3.3: Tambahkan *rate limiting* pada modul pemanggilan LLM.
  - [ ] SubTask 3.4: Tangkap *error* TLS pada MCP client dan teruskan pesan error tersebut ke antarmuka TUI/Operator (misal melalui *banner queue*).

# Task Dependencies
- [Task 2] bergantung pada perbaikan state TUI dan VIL secara umum.
- [Task 3] dapat dikerjakan secara paralel dengan Task 1 dan Task 2.
