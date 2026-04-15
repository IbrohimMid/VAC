# WAVE 4 BLUEPRINT — VAC Product Blueprint Spec

## Why
VAC perlu bertransformasi dari sekadar codebase implementasi menjadi produk *agentic development* yang matang, dapat dioperasikan, dapat diaudit, dan layak bersaing dengan produk lain (seperti Claude Code atau Stakpak). Wave 4 bertujuan untuk mendefinisikan posisi VAC sebagai *VIL-native autonomous development engine*, membedakannya melalui kedisiplinan eksekusi semantik, batasan kepercayaan (*trust boundary*), dan *controlled autonomy*, bukan sekadar *prompt orchestration*.

## What Changes
- Mendefinisikan Visi Produk dan *Positioning* (VAC sebagai agen pengkodean otonom VIL-native dengan *semantic planning* dan kontrol ketat).
- Menetapkan Persona Pengguna (*Solo technical operator*, *Builder/maintainer agentic system*, Tim *engineering* kecil).
- Menyusun *Product Principles* (VIL-first, *Operator clarity*, *Trust boundary explicit*, *Resume-safe autonomy*, *Delegation without chaos*, *Artifacts over vibes*, *Honest claims*).
- Menentukan *Product Scope* dalam 5 domain: *Product Surface*, *Core Agent Runtime*, *Trust and Safety Layer*, *Multi-agent / Delegation Layer*, dan *Repo Intelligence Layer*.
- Merancang *Capability Blueprint* dan *Architecture Blueprint* (termasuk kontrak privasi dan *approval*).
- Menyusun *Roadmap* Eksekusi Wave 4 (Milestone 4A hingga 4E).
- Menetapkan kriteria penerimaan (*Acceptance Criteria*) produk, teknis, dan kompetitif.

## Impact
- Affected specs: Roadmap produk keseluruhan, standar penerimaan kualitas, dan fokus pengembangan fitur mendatang.
- Affected code: Arsitektur level tinggi yang akan memandu perubahan pada `vac_core`, `vil_swarm`, `vac_tools`, dan `vac_cli`. Dokumentasi produk (mis. README dan panduan arsitektur).

## ADDED Requirements
### Requirement: VIL-Native Semantic Execution
Sistem HARUS mengutamakan eksekusi berbasis *typed Intermediate Representation* (IR) dan perencanaan semantik yang dapat diaudit, dibandingkan orkestrasi teks biasa.

#### Scenario: Subagent Delegation
- **WHEN** Agen mendelegasikan tugas ke subagen di dalam *sandbox*.
- **THEN** Subagen harus mewarisi *privacy vault* secara aman, beroperasi tanpa kebocoran variabel lingkungan induk, dan secara otomatis ditolak (Deny) jika mencoba mengakses alat yang memerlukan persetujuan (*approval*).

### Requirement: Resume-Safe Autonomy
Sistem HARUS mampu menyimpan *state* yang kritis secara aman dan memulihkannya setelah jeda atau interupsi.

#### Scenario: Session Checkpoint and Resume
- **WHEN** Operator memulihkan sesi agen dari sebuah *checkpoint*.
- **THEN** Sistem harus mengembalikan semua daftar alat yang telah disetujui (`approved_tools`) dan *state* kritis lainnya secara persis seperti sebelum sesi dihentikan.

## MODIFIED Requirements
### Requirement: Operator Approval Flow
Alur persetujuan (*approval flow*) HARUS menjadi kontrak *runtime* yang solid, bukan sekadar elemen antarmuka pengguna (UI). Status persetujuan harus tercatat pada *checkpoint metadata* dan mematuhi batas *sandbox*.