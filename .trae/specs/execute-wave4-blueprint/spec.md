# Execute Wave 4 Blueprint Spec

## Why
Wave 4 Blueprint telah mendefinisikan peta jalan untuk mentransformasi VAC menjadi produk *agentic development* yang otonom, andal, dan aman. Setelah menyelesaikan fase 4A (Product Surface) dan 4B (Trust & Safety), saatnya mengeksekusi fase 4C (Core Autonomy), 4D (Semantic Differentiation), dan 4E (Product Hardening) secara komprehensif hingga produk ini mencapai standar produksi dan siap diklaim sebagai solusi VIL-native yang lengkap.

## What Changes
- Mengimplementasikan pematangan otonomi inti (*Core Autonomy Maturity* - Wave 4C): peningkatan stabilitas *planning loop*, *validation loop*, dukungan pembatalan yang aman (*cancellation discipline*), dan *auditability* (log riwayat agen).
- Mengintegrasikan diferensiasi semantik (*Semantic Differentiation* - Wave 4D): penerapan *VIL-native planning*, validasi berbasis *Intermediate Representation* (IR), integrasi repositori pengetahuan (*knowledge*), dan *rulebook*.
- Melakukan penguatan produk (*Product Hardening & Packaging* - Wave 4E): pemolesan konfigurasi, optimalisasi mode *headless*/*autopilot*, alur instalasi, *onboarding*, dan dokumentasi akhir untuk rilis.

## Impact
- Affected specs: Roadmap Wave 4 (4C, 4D, 4E) dan standar operasional agen.
- Affected code: `vac_core`, `vil_swarm`, `vil_ir`, `vac_tools`, `vac_cli`, dan seluruh dokumen pendukung di dalam `docs/` serta konfigurasi produk (contoh: `vac.toml`).

## ADDED Requirements
### Requirement: Pematangan Otonomi (Core Autonomy)
Sistem HARUS mampu mempertahankan siklus perencanaan dan eksekusinya tanpa mengalami kerusakan data jika dibatalkan (*cancelled*) atau dihentikan sejenak (*paused*).

#### Scenario: Cancellation and Resume
- **WHEN** Operator membatalkan eksekusi panjang agen melalui input (misal: Ctrl+C)
- **THEN** Agen menghentikan aktivitas dengan aman, menyimpan *checkpoint*, dan dapat di-*resume* dengan *state* yang tidak korup.

### Requirement: Diferensiasi Semantik (Semantic Validation)
Sistem HARUS menggunakan pemahaman VIL IR untuk memvalidasi perubahan kode, alih-alih sekadar membaca teks.

#### Scenario: Code Refactoring Validation
- **WHEN** Agen selesai mengubah struktur kode (*refactoring*)
- **THEN** Agen menjalankan *validation pass* berbasis *IR validator* untuk memeriksa potensi kesalahan *handler* atau *pipeline*, sebelum melanjutkannya atau memintakan *approval*.

## MODIFIED Requirements
### Requirement: Product Hardening & Packaging
Pengaturan agen dan instalasi HARUS menjadi sederhana, didukung dengan dokumentasi yang operasional (instalasi, penggunaan *headless*, dll) sehingga operator non-pengembang dapat memakainya secara langsung.