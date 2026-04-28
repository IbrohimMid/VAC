# Cetak Biru Paritas Codex-Grade VAC TUI (C-TRACK)

> **C-TRACK:** Jalur strategis untuk menutup kesenjangan kapabilitas antarmuka dan pengalaman pengguna antara VAC dan TUI standar industri (Codex) tanpa mengubah kepemilikan inti semantik VAC.

Dokumen ini memetakan 12 inisiatif utama (C1-C12) yang diorganisir dalam empat "pilar" (Tentpoles) arsitektur utama untuk mencapai tingkat paritas *Codex-Grade*.

## Pilar A: Loop Agen Berbasis Aliran (The Agent Loop, Streamed)

**Tesis:** Kesenjangan fungsional terbesar adalah loop pemrosesan tunggal. Mesin pemrosesan harus beralih ke *async stream* blok alat untuk membuka kapabilitas agen otonom.

* **C1: Streaming Tool-Call Loop.** Mengonversi `submit_one` dari *single round-trip* menjadi `async_stream::Stream` yang mengirimkan *chunk* (Teks, Pemanggilan Alat, Hasil Alat, Selesai/Batal) secara *real-time*. Ini memerlukan fungsi validasi gerbang per-blok (`wrappedCanUseTool`).
* **C2: Kompaksi Konteks Otomatis.** Mekanisme *circuit breaker* yang dipicu secara otomatis saat sesi mendekati batas konteks model (contoh: `effectiveContextWindow - 13000`), menjalankan peringkasan latar belakang tanpa merusak aliran.
* **C3: Tampilan Tugas di Jalur Percakapan.** Mengangkat alat `TodoWrite` menjadi komponen visual *first-class* (widget `tasks`) di jalur percakapan, sehingga agen dapat mengomunikasikan rencana kerja kepada operator.

## Pilar B: Subagen yang Terlihat (Subagents, Visible)

**Tesis:** Eksekusi internal agen kelompok (`vil_swarm`) harus direpresentasikan secara eksplisit sebagai subagen yang dapat dilihat, dilacak, dan divalidasi oleh operator.

* **C4: Agen Perangkat *First-Class*.** Pembuatan `AgentTool` (Explore, Plan, Verify) yang dapat diinisiasi baik oleh operator maupun model, memunculkan sesi *sidechain* dengan kontrol izin mandiri yang terisolasi (contoh: mode *worktree* mandiri).
* **C5: Registri *Skills* Berbasis Markdown.** Implementasi pemuatan kemampuan tambahan melalui pustaka *skills* `.vac/skills/*.md`. Mengaktifkan pemanggilan fungsionalitas dengan perintah `/skills` dan metadata YAML (pencocokan nama, pemicu, perangkat).
* **C6: Pembatas Mode Rencana (*Strict Plan Mode Gate*).** Implementasi mode perencanaan khusus (melalui `EnterPlanModeTool`) yang secara proaktif memblokir pemanggilan alat destruktif hingga rencana kerja disetujui, direpresentasikan dalam antarmuka UI dengan lencana `plan`.

## Pilar C: Loop Otonom (Autonomous Loops)

**Tesis:** Menambahkan penjadwalan dan *hooks* untuk mengubah VAC dari sekadar sesi interaktif menjadi entitas agen persisten.

* **C7: Primitif Cron & Alat Pemantau.** Alat bawaan (`CronCreate`, `CronList`, `MonitorTool`) yang berjalan di latar belakang (simpanan `.vac/cron.json`), menggunakan *tokio task* untuk memicu tugas otomatis, memberikan umpan balik pada *sub-pane* baru di dalam *tab Runtime*.
* **C8: Penjadwalan Sinkronisasi (Pacing).** Implementasi perintah `ScheduleWakeup` dan pemicu putaran mandiri (`/loop`) untuk ritme iteratif yang terukur dari agen, menjaga ketahanan sesi dalam skenario panjang.
* **C9: Sistem Hook Dapat Dikonfigurasi.** Registri dengan dukungan 9 *event hook* standar dan 4 tipe perintah. Integrasi filter konfigurasi operator untuk memberikan kendali intersep (misal: sebelum alat digunakan, saat subagen berhenti).

## Pilar D: Sesi Portabel (Portable Sessions)

**Tesis:** Ergonomi dan visibilitas sesi tidak boleh dibatasi oleh satu terminal. Sesi harus portabel dan transparan secara token.

* **C10: Ekstensi Web & Worktree.** Pengenalan alat utilitas eksternal seperti `WebFetchTool` dan `WebSearchTool` serta utilitas `EnterWorktreeTool` yang dipetakan pada status direktori sesi untuk menangani repositori secara aman.
* **C11: Jembatan Jarak Jauh & Kustomisasi.** Ekspansi `vac_bridge` untuk mendukung arsitektur otentikasi JWT dan koneksi *websocket* (SSE) guna mengaktifkan fungsionalitas `/teleport` antar sesi perangkat, serta perintah tata letak seperti `/statusline` dan `/output-style`.
* **C12: UX Putar Ulang & Inspektur Konteks.** Alat navigasi historis (`/rewind`, `/thinkback`) dan alat analitik token (`/context`) yang menampilkan *bar chart* terperinci berbasis tipe pesan (sistem, operator, alat) sebagai *overlay*.

## Prinsip UX Utama (Non-Negotiable)
Setiap inisiatif C1-C12 wajib mematuhi:
1. Visibel di UI utama (tidak ada *state* yatim/hilang).
2. Kesalahan dilempar ke sistem *routing* notifikasi.
3. Menggunakan bentuk tata letak UI yang sudah ada (tidak membuat kelas *overlay* baru).
4. Dapat dijangkau melalui palet perintah (`ActionSpec`).
5. Mempertahankan struktur skrip histori asli.
