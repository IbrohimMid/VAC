# Address VAC Audit Findings Spec

## Why
Berdasarkan hasil audit independen terhadap branch `main` terbaru, telah diidentifikasi bahwa pembaruan besar (batch) berhasil membawa peningkatan substantif, seperti Unit 6 (Shell polish) dan Unit 8 (Banner queue) yang mendapatkan status PASS kuat. Namun, terdapat beberapa temuan krusial yang perlu ditangani. Pertama, ada masalah rekonsiliasi state pada Unit 7 terkait field `ask_user` yang tidak selaras di `types.rs`. Kedua, Unit 9 mengenai "VIL Workstation" masih berstatus PARTIAL dan overclaim karena tidak memiliki representasi tab yang berdedikasi. Selain itu, berdasarkan analisis kesiapan produksi (skor 78/100), ada beberapa celah kritis yang harus segera ditangani, meliputi buffer output shell yang tidak terbatas, deteksi secret yang naif, ketiadaan rate limiting pada panggilan LLM, dan kegagalan MCP TLS yang tidak tersampaikan ke operator.

## What Changes
- Memperbaiki inkonsistensi state model pada Unit 7 (`ask_user_question_kind`, `ask_user_multi_selected`, `ask_user_metadata` di `types.rs`).
- Mengembangkan dan mendedikasikan tab workstation VIL secara penuh (misal: `WorkbenchTab::Vil`) untuk menyelesaikan target Unit 9.
- Mengimplementasikan batasan ukuran (bounded) atau circular buffer pada output shell.
- Meningkatkan algoritma deteksi secret menjadi lebih tangguh (robust).
- Menambahkan mekanisme rate limiting untuk pemanggilan LLM.
- Menampilkan error dan kegagalan MCP TLS secara eksplisit melalui TUI kepada operator.

## Impact
- Affected specs: TUI UX, VIL Workstation, Shell Management, LLM Integration, MCP Client
- Affected code: `types.rs` (dan/atau file state terkait), modul shell, modul pendeteksi secret, LLM service, MCP client, dan antarmuka tab VIL.

## ADDED Requirements
### Requirement: Pengendalian Buffer Shell
Sistem HARUS membatasi ukuran buffer output shell agar tidak menggunakan memori tanpa batas.

#### Scenario: Shell menghasilkan output sangat panjang
- **WHEN** proses shell mencetak output yang sangat panjang
- **THEN** sistem akan membuang output terlama atau menyimpannya dalam batasan buffer yang ditentukan (misalnya circular buffer) sehingga memori tidak habis.

### Requirement: Rate Limiting LLM
Sistem HARUS membatasi jumlah pemanggilan LLM dalam jangka waktu tertentu untuk mencegah penyalahgunaan atau lonjakan biaya (rate limiting).

### Requirement: Notifikasi Error MCP TLS
Sistem HARUS memunculkan peringatan atau banner kepada operator saat terjadi kegagalan TLS pada koneksi MCP, agar operator mengetahui masalah koneksi secara real-time.

## MODIFIED Requirements
### Requirement: Ask-User State
State `ask_user` pada `types.rs` HARUS secara eksplisit mendefinisikan field `ask_user_question_kind`, `ask_user_multi_selected`, dan `ask_user_metadata` agar selaras dengan implementasi service `ask_user.rs` dan mencegah inkonsistensi.

### Requirement: VIL Workstation
Workstation VIL HARUS memiliki representasi tab yang berdedikasi (seperti `WorkbenchTab::Vil`) dan tidak hanya tersebar di bagian lain, guna memenuhi klaim "VIL Workstation complete".

## REMOVED Requirements
Tidak ada requirement yang dihapus pada iterasi ini.