# Tasks

- [x] Task 1: Eksekusi Wave 4C - Pematangan Otonomi Inti (Core Autonomy)
  - [x] SubTask 1.1: Perkuat *planning loop* dan *validation loop* di dalam `vac_core` dan `vil_swarm` (penanganan error yang lebih elegan dan log audibilitas).
  - [x] SubTask 1.2: Pastikan fitur pembatalan (*cancellation discipline*) dan *resumability* berjalan aman, sehingga *state* tersimpan utuh di *checkpoint* saat interupsi.
- [x] Task 2: Eksekusi Wave 4D - Diferensiasi Semantik (Semantic Differentiation)
  - [x] SubTask 2.1: Integrasikan *VIL-native planning* dan pemahaman IR (`vil_ir`) ke dalam proses validasi agen (`vac_core/src/engine.rs`).
  - [x] SubTask 2.2: Integrasikan profil repositori (`vil_knowledge` dan *rulebook*) sebagai konteks aktif pada saat eksekusi.
- [x] Task 3: Eksekusi Wave 4E - Product Hardening & Packaging
  - [x] SubTask 3.1: Tambahkan dukungan penuh dan optimalisasi untuk mode eksekusi *headless* dan *autopilot* di `vac_cli`.
  - [x] SubTask 3.2: Buat dan sempurnakan dokumentasi instalasi, konfigurasi awal (`vac.toml`), serta `docs/onboarding.md` untuk pengguna baru.

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 2]