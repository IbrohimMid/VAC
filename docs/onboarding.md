# Panduan Onboarding VAC

Selamat datang di VAC (Vastar Agentic CLI)! Dokumen ini akan memandu Anda melalui proses instalasi, konfigurasi awal, serta penggunaan perintah-perintah dasar agar Anda dapat segera mulai menggunakan agen otonom ini.

## 1. Instalasi

Untuk menginstal VAC, Anda harus memiliki lingkungan pengembangan Rust (`cargo`) yang sudah terinstal di sistem Anda.

Jalankan perintah berikut di dalam direktori repositori VAC untuk melakukan kompilasi dan instalasi:

```bash
cargo install --path crates/vac_cli --locked
```

Atau jika Anda sudah berada di dalam repositori utama:
```bash
cargo install --path crates/vac_cli --locked
```

Pastikan direktori instalasi `cargo` (biasanya `~/.cargo/bin`) sudah berada di dalam `PATH` sistem Anda.

## 2. Konfigurasi `vac.toml`

VAC menggunakan file konfigurasi (umumnya di `.vac/config.toml` atau global di `~/.config/vac/config.toml`, sering juga direferensikan sebagai `vac.toml`) untuk mengatur provider LLM, token limit, dan izin akses *tool*.

Berikut adalah contoh konfigurasi dasar yang dapat Anda gunakan:

```toml
[llm]
default_provider = "anthropic"
budget_tokens = 0

# Konfigurasi Provider Anthropic
[llm.providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-3-5-sonnet-20241022"
max_tokens = 8192
temperature = 0.0

# Konfigurasi Provider OpenAI (Opsional)
[llm.providers.openai]
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o"
max_tokens = 8192
temperature = 0.0

[tools]
default_policy = "deny"

# Daftar tool yang diizinkan untuk digunakan oleh agen
[tools.allow]
bash = true
cargo = true
file_edit = true
file_read = true
file_write = true
git = true
glob = true
grep = true
search = true
task_done = true
todo_write = true

[runtime]
enable = false
task_intent_mode = "monitor-only"
environment_mode = "host"
execution_environment = "host"
network_policy = "inherit"
max_concurrent_jobs = 2

# Optional production/operator isolation:
# container_runtime = "docker"
# container_image = "ghcr.io/your-org/vac-runtime:latest"
# allowed_mounts = ["."]
# allowed_env = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]
```

### Mengatur API Keys

Sesuai dengan konfigurasi `api_key_env` di atas, VAC akan mencari *environment variables* untuk mendapatkan kunci API. Anda perlu mengekspornya di shell Anda (atau menambahkannya ke `.bashrc`/`.zshrc`):

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-proj-..."
```

## 3. Perintah Dasar VAC

Setelah instalasi dan konfigurasi selesai, berikut adalah perintah-perintah utama untuk berinteraksi dengan VAC:

### `vac init`
Menginisialisasi VAC di dalam proyek Anda. Perintah ini akan membuat direktori `.vac/` di dalam *root* proyek yang berisi file konfigurasi default dan struktur direktori yang dibutuhkan oleh VAC (seperti *memory*, *traces*, dan *sessions*).
```bash
vac init
```

### `vac run`
Menjalankan agen untuk mengerjakan satu tugas atau instruksi secara langsung dari terminal. VAC akan membaca instruksi, merencanakan eksekusi, memanggil *tools*, dan menyelesaikan tugas secara otonom.

Anda dapat menggunakan flag `--approve` untuk menyetujui semua operasi secara otomatis (Headless Mode) tanpa meminta konfirmasi dari Anda:
```bash
vac run "Tambahkan error handling pada modul authentikasi" --approve
```

### `vac resume`
Memulihkan (*resume*) status dari sesi yang tertunda atau terhenti. Jika Anda menjalankan ini tanpa UI interaktif, agen akan melanjutkan eksekusi di latar belakang:
```bash
vac resume <session-id>
```
Sangat disarankan untuk melanjutkan sesi melalui menu Resume di dalam `vac interactive` agar Anda tetap bisa memantau *tools* dan memberikan persetujuan (*approval*).

### `vac interactive`
Membuka *Terminal User Interface* (TUI) interaktif. Ini adalah **mode yang sangat direkomendasikan** karena menyediakan antarmuka visual untuk melihat *streaming* pemikiran agen, *progress* eksekusi, serta memberikan persetujuan (*approval*) secara langsung jika agen ingin mengeksekusi perintah bash atau operasi file yang berisiko tinggi.
```bash
vac interactive
```

### `vac autopilot up|down|status`
Mengelola *daemon* autopilot di latar belakang. Mode ini memungkinkan VAC memonitor sistem (perubahan file, *cron jobs*, atau antrean tugas) secara otomatis.
```bash
# Menjalankan autopilot
vac autopilot up

# Melihat status
vac autopilot status

# Menghentikan autopilot
vac autopilot down
```

### `vac runtime status|jobs|inspect|cancel|retry`
Melihat runtime/operator plane secara langsung dari CLI. Surface ini menampilkan *task intent mode*, *environment mode*, execution boundary, antrean job, dan state autopilot.
```bash
vac runtime status
vac runtime jobs
vac runtime inspect <job-id>
vac runtime cancel <job-id>
vac runtime retry <job-id>
```

### `vac isolation status|logs|run`
Memeriksa isolation boundary dan menjalankan command di container runtime ketika `execution_environment` diset ke mode isolated.
```bash
vac isolation status
vac isolation logs
vac isolation run -- bash -lc "pwd && ls"
```

### Surface Operator TUI
Di `vac interactive`, workbench sekarang mencakup tab `Runtime` dan shell operator PTY:

- `/runtime` membuka tab runtime jobs
- `/shell <cmd>` menjalankan shell interaktif
- `/shell-bg` membackground shell
- `/shell-focus` memfokuskan kembali shell
- `/shell-kill` menghentikan shell

Jika `execution_environment = "isolated_interactive"`, shell TUI akan berjalan di dalam container boundary, bukan host shell.

---

Sekarang Anda sudah siap untuk mengeksplorasi kemampuan VAC sebagai agen *pair-programmer* Anda! Untuk referensi lebih lanjut mengenai arsitektur privasi, silakan baca `privacy_architecture.md`.
