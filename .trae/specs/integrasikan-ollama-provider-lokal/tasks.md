# Tasks
- [x] Task 1: Definisikan surface konfigurasi provider Ollama
  - [x] Tambahkan schema config minimal untuk provider `ollama` (base_url, model, timeouts, parameter generasi dasar).
  - [x] Pastikan `vac exec` dan `vac chat` dapat memilih provider `ollama` via flag/config tanpa hardcode provider mock.

- [x] Task 2: Implement adapter/provider `ollama` dengan streaming
  - [x] Implement request prompt → stream delta teks.
  - [x] Implement mapping error yang actionable (server down, model missing).
  - [x] Pastikan tidak meng-log secrets dan tidak memasukkan payload sensitif ke telemetry/hook.

- [ ] Task 3: Integrasi UX minimal (CLI + TUI)
  - [ ] CLI: tampilkan provider/model yang aktif pada output ringkas (yang sudah ada mekanismenya).
  - [ ] TUI: statusline menampilkan provider/model bila tersedia (tanpa menambah panel baru).

- [x] Task 4: E2E dogfood script (tanpa menambah dokumen baru)
  - [x] Tambahkan helper test/integration yang menjalankan jalur provider Ollama (router/provider) dengan prompt deterministik.
  - [x] Test HARUS auto-skip jika Ollama tidak tersedia.

- [ ] Task 5: Verifikasi end-to-end di sandbox dev
  - [x] Instal Ollama di sandbox dev dan `ollama pull qwen2.5-coder` (atau varian yang disepakati).
  - [ ] Jalankan `ollama run qwen2.5-coder` untuk warmup, lalu jalankan `vac exec` dan `vac chat` untuk memverifikasi streaming + tool cards.
  - [x] Jika model default tidak bisa jalan karena limit memory sandbox, gunakan varian yang lebih kecil dan catat constraint-nya (sandbox ini dibatasi ~3 GiB; `qwen2.5-coder` butuh >4 GiB).

# Task Dependencies
- Task 2 bergantung pada Task 1 (config & provider selection).
- Task 3 bergantung pada Task 2 (provider label tersedia).
- Task 4 bergantung pada Task 2.
- Task 5 bergantung pada Task 2 (provider berfungsi).
