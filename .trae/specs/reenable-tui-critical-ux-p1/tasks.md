# Tasks
- [x] Task 1: Konfirmasi titik integrasi dan adapter contract untuk services Stakpak yang akan diaktifkan
  - [x] Identifikasi state/event yang dibutuhkan oleh: `approval_bar`, `model_switcher`, `file_search`, `toast`, `changeset`
  - [x] Definisikan contract adapter minimal untuk memetakan tipe VAC/VIL (tool call, approval request, file ops) ke tipe yang dipakai services
  - [x] Pastikan build graph tidak bergantung pada `stakpak_*` crates (atau dibatasi pada modul isolasi bila memang sudah ada)

- [x] Task 2: Re-enable Toast Notifications
  - [x] Aktifkan service `toast` dan event hook untuk minimal: model switch, approval decision, dan error runtime
  - [x] Pastikan toast tidak menutupi input, dan mengikuti theme yang ada

- [x] Task 3: Re-enable Rich Approval Bar
  - [x] Aktifkan service `approval_bar` dan integrasikan dengan alur approval VAC yang sekarang
  - [x] Implementasi jalur preview ringkas (minimal: daftar file terkait + ringkasan diff jika tersedia)
  - [x] Pastikan accept/reject menghasilkan decision yang dikonsumsi runtime dan state tersinkron

- [x] Task 4: Re-enable Interactive Model Switcher
  - [x] Aktifkan service `model_switcher` dan wiring ke konfigurasi model VAC
  - [x] Pastikan perubahan model berlaku untuk request berikutnya tanpa restart sesi
  - [x] Tambahkan shortcut/palette entry untuk membuka switcher

- [x] Task 5: Re-enable File Search & Navigation
  - [x] Aktifkan service `file_search` dan implementasi indexing minimal untuk workspace
  - [x] Pastikan pencarian fuzzy bekerja dan selection memicu preview/action yang disepakati (minimal: tampilkan path + opsi open diff)

- [x] Task 6: Re-enable Changeset Tracking & Review Popup
  - [x] Aktifkan service `changeset` dan integrasikan dengan pipeline perubahan file VAC (diff/patch/apply)
  - [x] Pastikan UI changeset dapat dibuka untuk melihat daftar file berubah dan preview diff ringkas
  - [x] Pastikan changeset dapat dipakai sebagai sumber preview untuk approval bar

- [x] Task 7: Validasi end-to-end dan hardening
  - [x] Tambahkan/rapikan test yang sudah ada (atau minimal smoke test runnable) untuk memastikan TUI dapat start
  - [x] Verifikasi semua shortcut/palette actions tidak crash pada state kosong
  - [x] Pastikan tidak ada logging secret/credential dalam event/toast/error rendering

# Task Dependencies
- Task 2 depends on Task 1
- Task 3 depends on Task 1
- Task 4 depends on Task 1
- Task 5 depends on Task 1
- Task 6 depends on Task 1
- Task 7 depends on Task 2, Task 3, Task 4, Task 5, Task 6
