# Arsitektur Privasi dan Kontrak Eksekusi

Dokumen ini menjelaskan arsitektur aktual dan alur *source of truth* dari perlindungan privasi yang membentang di seluruh ekosistem `vac_core`, `vil_swarm`, dan `vac_tools`.

## 1. Alur *Source of Truth* Privasi (Source of Truth Privacy Flow)
`PrivacyVault` (di dalam `vac_tools::privacy`) adalah pusat *source of truth* untuk seluruh proses perlindungan *secret* dalam aplikasi. Saat pengguna memulai instruksi (*task description*), `VacEngine` akan secara instan melakukan `substitute` pada instruksi tersebut. Dengan demikian, sebelum instruksi ini menyentuh *agent loop* atau diteruskan ke *model provider*, setiap kredensial atau pola yang sensitif telah diamankan menggunakan alias yang bersifat *opaque* (seperti `SECRET_API_KEY_1`).

## 2. Batas Substitusi vs Restorasi (Substitution vs Restore Boundary)
Batas privasi didefinisikan secara ketat pada titik masuk dan titik keluar interaksi LLM:
- **LLM Boundary**: Hanya melihat dan menghasilkan teks bersubstitusi (*alias placeholder*).
- **User/UI Boundary**: Setiap respon (*assistant chunk*) dari LLM yang akan ditampilkan ke UI akan melewati proses `restore` di dalam `VacEngine`, sehingga *user* selalu melihat teks asli dari *secret* tersebut.
- **Tool Execution Boundary**: *Tool Router* (`vac_tools::router`) memiliki akses ke `PrivacyVault`. Sebelum *tool* dieksekusi, parameter/argumen *JSON* yang diberikan LLM akan di-`restore` menjadi nilai aslinya. Segera setelah eksekusi *tool* selesai, nilai *return* (baik *stdout*, *stderr*, atau objek JSON) akan di-`substitute` kembali agar LLM tidak melihat *secret* di hasil eksekusi *tool*.

## 3. Jalur Engine -> Swarm -> Tool Router
Alur pendistribusian konfigurasi privasi terjadi secara referensial, bukan dengan menyalin *state* (*zero-copy untuk state sinkronisasi*):
1. `VacEngine` di `vac_core` membuat `Arc<RwLock<PrivacyVault>>` pada saat inisialisasi awal.
2. Referensi *vault* ini diinjeksi (*passed down*) ke `SwarmOrchestrator` (`vil_swarm`).
3. Saat *agent* atau *subagent* melakukan proses pembuatan konteks (*tool context*), referensi *vault* ini dimasukkan ke dalam `ToolContext`.
4. Akhirnya, `ToolRouter` yang memanggil metode registri membaca (`read().await`) untuk restorasi dan memodifikasi (`write().await`) untuk mendaftarkan substitusi alias baru jika ditemukan *secret* yang belum terpetakan.

## 4. Persistensi Persetujuan Checkpoint (Checkpoint Approval Persistence)
Kontrak status persetujuan (*approval state*) diekspor langsung ke *checkpoint* (`AgentRunState`). Persetujuan manusia disimpan di *memory* sebagai struktur `HashSet<String>` yang merekam *ID* dari panggilan *tool* yang telah disetujui (dikenal dengan variabel `approved_tools`).
Saat `vac_core` atau VIL menyimpan *state checkpoint* ke piringan (disk), sekumpulan ID ini ikut diserialisasi. Saat proses *restore session* terjadi dari piringan, persetujuan akan dimuat ulang ke dalam *state*. Akibatnya, *tool* yang sebelumnya sudah disetujui sebelum *restart* tidak akan meminta persetujuan lagi.

## 5. Sandboxed Subagent Deny-on-Needs-Approval
Ketika sub-agen (subagent) berjalan di lingkungan *sandboxed* (yaitu dengan status `AgentZone::SandboxedSubagent`), ada kontrak eksplisit pada *router*: 
Jika sebuah *tool* membutuhkan persetujuan (*policy decision* mengevaluasi ke `NeedsApproval`), router tidak akan menghentikan alur untuk meminta persetujuan manusia, melainkan akan **langsung memblokir eksekusi tersebut (deny)** dan mengembalikan galat `PermissionDenied`. Hal ini menjamin bahwa *subagent* yang terisolasi secara rekursif tidak akan dapat memicu blokir interaktif persetujuan dari UI utama.

## 6. Keterbatasan yang Diketahui (Remaining Known Limitations)
Berikut adalah beberapa batasan pada implementasi arsitektur privasi ini:
1. **Pola RegEx Statis**: Mekanisme identifikasi *secret* masih sepenuhnya bergantung pada pola *regex* statis secara *hardcoded* (`aws_key`, `api_key`, `bearer`, `ip`, dan `aws_account`). *Secret* yang memiliki format acak atau yang tidak sesuai dengan pola bawaan ini tidak akan dideteksi dan dapat terekspos.
2. **Konteks Buta (Context-Blind Substitution)**: Proses substitusi tidak melihat konteks semantik dari teks (misalnya, *IP address* dari tutorial atau `127.0.0.1` akan dienkripsi tanpa pandang bulu menjadi `SECRET_IP_N`).
