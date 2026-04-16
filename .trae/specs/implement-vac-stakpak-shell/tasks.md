# Tasks

## Branch A: `tui-product-shell-parity`
- [x] Task 1: Side Panel — State & Layout Architecture
  - [x] Tambahkan `side_panel_visible`, `side_panel_width`, `side_panel_section_collapsed` ke `AppState`
  - [x] Tambahkan enum `SidePanelSection`
  - [x] Update `view.rs` untuk split workspace horizontal dan layout panel
  - [x] Tambahkan keybind `Ctrl+B` untuk toggle side panel
  - [x] Adaptasi logic layout dari `services_stakpak_disabled/side_panel.rs`
- [x] Task 2: Side Panel — Context Section
  - [x] Buat `crates/vac_cli/src/tui/services/side_panel.rs`
  - [x] Implementasi `render_context_section()` menggunakan data `current_model`, `session_id`, `loading`, `auto_approve`
- [x] Task 3: Side Panel — Runtime Section
  - [x] Implementasi `render_runtime_section()` untuk menampilkan jobs per status dari `AppState.runtime_jobs`
- [x] Task 4: Side Panel — Changeset Section
  - [x] Implementasi `render_changeset_section()` dari `AppState.modified_files` (max 8 entries)
- [x] Task 5: Profile & Rulebook Switcher
  - [x] Tambahkan state terkait profile dan rulebook switcher ke `AppState`
  - [x] Adaptasi `services_stakpak_disabled/profile_switcher.rs` dan `rulebook_switcher.rs`
  - [x] Implementasi keybind `Ctrl+O` dan `Ctrl+R`
  - [x] Tampilkan badge profil aktif di header bar
- [x] Task 6: Message Action Popup
  - [x] Tambahkan `message_action_popup` state ke `AppState`
  - [x] Adaptasi `services_stakpak_disabled/message_action_popup.rs` dengan opsi: Copy content, Retry from here, Dismiss
  - [x] Implementasi keybind `a` pada message hover

## Branch B: `isolation-ux-parity`
- [x] Task 7: `vac isolation wrap` — Arbitrary Command Path
  - [x] Tambahkan `Wrap` ke `IsolationAction` enum
  - [x] Implementasi `execute_wrap()` di `commands/isolation.rs` untuk menjalankan command via `build_container_command`
- [x] Task 8: `vac isolation wrap` — Re-exec VAC Path
  - [x] Update `execute_wrap()`: jika `args.is_empty()`, mount VAC binary dan exec `vac interactive`
  - [x] Tambahkan env var `VAC_INSIDE_ISOLATION=1` dan tampilkan banner di TUI header
- [x] Task 9: `vac isolation clear-logs` dan `vac isolation doctor`
  - [x] Implementasi `ClearLogs` action untuk truncate `isolation.log_path()`
  - [x] Implementasi `Doctor` action untuk cek ketersediaan runtime, image, mounts, dll
- [x] Task 10: Default Mount Presets
  - [x] Tambahkan auto-include `~/.cargo/registry` dan `~/.rustup` di `IsolationManager::resolve_mounts()` jika terdeteksi Rust project
  - [x] Buat enum `MountPreset` dan integrasikan ke `RuntimeConfig`

## Branch C: `mcp-admin-surface`
- [x] Task 11: MCP Runtime Connection Probe
  - [x] Tambahkan struct `McpConnectionState`
  - [x] Implementasi `probe_mcp_server()` untuk cek stdio transport dan HTTP HEAD untuk SSE
  - [x] Update `commands/mcp.rs` `status()` untuk menggunakan `probe_mcp_server()`
- [x] Task 12: MCP TUI Panel Badges
  - [x] Tambahkan `mcp_server_states` ke `AppState`
  - [x] Probe MCP servers saat startup di `event_loop.rs`
  - [x] Tampilkan ringkasan MCP servers di side panel Context section dan workbench Runtime tab
- [x] Task 13: MCP Trust Diagnostics
  - [x] Update `commands/mcp.rs` `list()` untuk menampilkan warning mismatch (trust_class, approval_policy)

## Branch D: `vil-native-operator-layer`
- [x] Task 14: VIL Status State di AppState
  - [x] Tambahkan `VilStatusSnapshot` ke `AppState`
  - [x] Jalankan `VilProjectProfile::detect()` di background task saat startup dan update state via event channel

- [x] Task 15: Side Panel — VIL Status Section
  - [x] Implementasi `render_vil_status_section()` di side panel dengan data real dari `VilStatusSnapshot`

- [x] Task 16: VIL Validation Wire ke Changeset
  - [x] Trigger re-validation dengan `vil_validate::validate_changes()` saat `AppEvent::ChangesetUpdated` diterima
  - [x] Update `VilStatusSnapshot` berdasarkan event hasil validasi

- [x] Task 17: VIL Status Badge di Header
  - [x] Tambahkan badge VIL ke `render_header()` di `view.rs` yang bisa diklik untuk toggle panel

## Branch E: `shell-polish`
- [x] Task 18: Shell Background/Refocus UX
  - [x] Tambahkan `ShellState::Backgrounded` dan mini-banner di footer
  - [x] Implementasi `Ctrl+Z` untuk toggle background/foreground shell
- [x] Task 19: Shell History
  - [x] Tambahkan state `shell_history` ke `AppState`
  - [x] Implementasi push history dan navigasi Up/Down di shell input

## Branch F: `product-polish`
- [x] Task 20: Reliability — Crash-safe Checkpoint
  - [x] Tambahkan periodic checkpoint write (setiap 30s) di `runner.rs`
  - [x] Dukung flag `--resume` di startup untuk merestore checkpoint state
- [x] Task 21: Doctor — Isolation & MCP Checks
  - [x] Update `commands/doctor.rs` untuk menggabungkan Isolation dan MCP checks

# Task Dependencies
- [Task 5] depends on [Task 1]
- [Task 8] depends on [Task 7]
- [Task 10] depends on [Task 7]
- [Task 12] depends on [Task 11]
- [Task 15] depends on [Task 14]
- [Task 16] depends on [Task 15]
- [Task 17] depends on [Task 16]
- [Task 14] depends on [Task 1, Task 2, Task 3, Task 4]
- [Task 21] depends on [Task 9, Task 11]
