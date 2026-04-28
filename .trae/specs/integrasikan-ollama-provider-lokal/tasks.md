# Tasks
- [ ] Task 1: Definisikan surface konfigurasi provider Ollama
  - [ ] Tambahkan schema config minimal untuk provider `ollama` (base_url, model, timeouts, parameter generasi dasar).
  - [ ] Pastikan `vac exec` dan `vac chat` dapat memilih provider `ollama` via flag/config tanpa hardcode provider mock.

- [ ] Task 2: Implement adapter/provider `ollama` dengan streaming
  - [ ] Implement request prompt → stream delta teks.
  - [ ] Implement mapping error yang actionable (server down, model missing).
  - [ ] Pastikan tidak meng-log secrets dan tidak memasukkan payload sensitif ke telemetry/hook.

- [ ] Task 3: Integrasi UX minimal (CLI + TUI)
  - [ ] CLI: tampilkan provider/model yang aktif pada output ringkas (yang sudah ada mekanismenya).
  - [ ] TUI: statusline menampilkan provider/model bila tersedia (tanpa menambah panel baru).

- [ ] Task 4: E2E dogfood script (tanpa menambah dokumen baru)
  - [ ] Tambahkan helper test/integration yang menjalankan jalur `vac exec --provider ollama` dengan prompt deterministik.
  - [ ] Test HARUS auto-skip jika Ollama tidak tersedia.

- [ ] Task 5: Verifikasi end-to-end di sandbox dev
  - [ ] Instal Ollama di sandbox dev dan `ollama pull qwen2.5-coder` (atau varian yang disepakati).
  - [ ] Jalankan `ollama run qwen2.5-coder` untuk warmup, lalu jalankan `vac exec` dan `vac chat` untuk memverifikasi streaming + tool cards.

# Task Dependencies
- Task 2 bergantung pada Task 1 (config & provider selection).
- Task 3 bergantung pada Task 2 (provider label tersedia).
- Task 4 bergantung pada Task 2.
- Task 5 bergantung pada Task 2 (provider berfungsi).

