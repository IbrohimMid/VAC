# VAC: Stakpak-Class Agent Shell + VIL-Native Engine Spec

## Why
VAC sudah memiliki fondasi runtime yang solid (isolation, MCP trust, shell PTY, approvals, changeset, sessions, runtime jobs), namun perlu meningkatkan breadth di sisi operator UX agar setara dengan product shell `stakpak/agent` dan bahkan melampauinya dengan VIL-native superpowers.

## What Changes
- Penambahan Side panel MVP dengan Context, Runtime, Changeset, dan VIL status.
- Implementasi Profile/rulebook/model switcher UX.
- Penambahan mode ganda untuk `vac isolation wrap`: arbitrary command dan re-exec VAC itself.
- Peningkatan MCP admin surface dengan pengecekan runtime connection state dan trust diagnostics.
- Integrasi VIL project awareness ke `VilProjectProfile::detect()` dan `vil_validate`.
- Adaptasi UX/layout architecture dari Stakpak (hanya layout, bukan semantics).

## Impact
- Affected specs: TUI Product Shell, Isolation UX, MCP Admin Surface, VIL-Native Operator Layer, Shell Polish, Product Polish.
- Affected code:
  - `AppState` (penambahan state untuk side panel, profile switcher, VIL status, dll)
  - `crates/vac_cli/src/tui/services/side_panel.rs`
  - `crates/vac_cli/src/tui/services/profile_switcher.rs`
  - `crates/vac_cli/src/tui/services/rulebook_switcher.rs`
  - `crates/vac_cli/src/tui/services/message_action_popup.rs`
  - `crates/vac_cli/src/commands/isolation.rs`
  - `crates/vac_cli/src/commands/mcp.rs`
  - `crates/vac_cli/src/commands/doctor.rs`
  - `crates/vac_cli/src/tui/view.rs`
  - `crates/vac_cli/src/tui/event_loop.rs`
  - `crates/vac_tools/src/mcp.rs`
  - `crates/vac_core/src/runner.rs`

## ADDED Requirements

### Requirement: TUI Product Shell Parity
Sistem SHALL menyediakan side panel yang collapsible dan menampilkan informasi kontekstual (Context, Runtime, Changeset, VIL Status).
Sistem SHALL menyediakan switcher untuk profile dan rulebook, serta popup aksi per-message.

#### Scenario: Toggle Side Panel
- **WHEN** user menekan `Ctrl+B`
- **THEN** side panel muncul di sebelah kanan layar, menampilkan section Context, Runtime, Changeset, dan VIL Status.

#### Scenario: Profile Switcher
- **WHEN** user menekan `Ctrl+O`
- **THEN** popup daftar profile muncul untuk dipilih, dan merubah badge di header bar setelah dipilih.

### Requirement: Isolation UX Parity
Sistem SHALL mendukung perintah `vac isolation wrap` dengan mode arbitrary command dan mode re-exec VAC.
Sistem SHALL menyediakan perintah `clear-logs` dan `doctor` untuk isolation.
Sistem SHALL menyediakan default mount presets berdasarkan project type.

#### Scenario: Arbitrary Command di Isolation
- **WHEN** user menjalankan `vac isolation wrap -- cargo test`
- **THEN** command berjalan di dalam container dengan mount yang telah dikonfigurasi.

### Requirement: MCP Admin Surface
Sistem SHALL memeriksa status koneksi runtime MCP server dan menampilkan trust diagnostics.

#### Scenario: Cek Status MCP
- **WHEN** user menjalankan `vac mcp status`
- **THEN** status koneksi aktual (connected/unreachable) dari setiap MCP server ditampilkan.

### Requirement: VIL-Native Operator Layer
Sistem SHALL mendeteksi status project VIL dan menampilkannya secara real-time di UI, termasuk validation score.

#### Scenario: VIL Status Badge
- **WHEN** VAC dijalankan di dalam project VIL
- **THEN** header dan side panel menampilkan badge VIL archetype beserta score validation terakhir.
