# Tasks
- [x] Task 1: Validasi Konstruktor SemanticChunker
  - [x] Tambahkan validasi `chunk_size > 0` dan `chunk_overlap < chunk_size` menggunakan `debug_assert!` di `crates/vil_context/src/chunking.rs`.
- [x] Task 2: Perbaiki Bug pada SemanticChunker::chunk
  - [x] Revisi fungsi `chunk` untuk menghindari loop tanpa akhir (infinite loop) dengan menetapkan perhitungan *sliding window* yang benar. Pastikan step selalu dijamin minimal 1.
  - [x] Sesuaikan logika perhitungan `byte_count` untuk menghitung spasi antar kata dengan tepat.
- [x] Task 3: Tambahkan Unit Test untuk Edge Cases
  - [x] Tambahkan module `tests` di dalam file tersebut.
  - [x] Tambahkan skenario `chunk_overlap_boundary_does_not_infinite_loop` (teks pas seukuran overlap + 1) dan kondisi rentan lainnya agar ter-cover dan tereksekusi tanpa menggantung.

# Task Dependencies
- Task 2 depends on Task 1
- Task 3 depends on Task 2