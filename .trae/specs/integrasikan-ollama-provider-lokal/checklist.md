- [ ] Provider `ollama` dapat dipilih via CLI tanpa hardcode provider mock.
- [ ] Konfigurasi `.vac/config.toml` mendukung `llm.providers.ollama` (base_url + model minimal) dan tervalidasi.
- [ ] `vac exec --provider ollama "<prompt>"` menampilkan output inkremental (stream), bukan rangkuman akhir.
- [ ] TUI `vac chat` (atau jalur interactive yang setara) menampilkan streaming text dan tool call cards muncul segera saat tool diminta.
- [ ] Error “ollama tidak berjalan” dan “model belum ada” bersifat actionable (instruksi langkah berikutnya jelas).
- [ ] Test provider Ollama auto-skip di CI ketika Ollama tidak tersedia (bukan fail).
- [ ] Tidak ada kebocoran payload sensitif ke hook env / log / telemetry pada jalur provider Ollama.

