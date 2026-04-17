# Rencana Migrasi UI Stakpak ke VAC

## Status Saat Ini

UI VAC masih menggunakan versi minimal karena komponen stakpak (43 files) yang sudah di-copy ke `services_stakpak_disabled/` belum diaktifkan.

### Service Aktif (4 files):
- `message.rs` - Rendering pesan dasar
- `detect_term.rs` - Deteksi terminal theme
- `textarea.rs` - Input text area sederhana
- `helper_block.rs` - Welcome messages

### Service Disabled (43 files):
Komponen canggih dari stakpak yang belum aktif:
- `markdown_renderer.rs` - Markdown rendering
- `syntax_highlighter.rs` - Syntax highlighting
- `approval_bar.rs` - HITL approval UI
- `file_diff.rs` - File diff viewer
- `side_panel.rs` - Side panel
- `bash_block.rs` - Bash output rendering
- `plan_review.rs` - Plan review UI
- Dan 36 komponen lainnya...

## Langkah Migrasi

### Fase 1: Persiapan (Prioritas Tinggi)
1. **Audit Dependencies**
   - Identifikasi semua `use stakpak_*` imports
   - Map ke tipe VAC yang ekuivalen
   - Buat stub types jika perlu

2. **Fix Type Definitions**
   - Update `AppState` references
   - Sesuaikan event types
   - Fix struct field names

### Fase 2: Aktivasi Komponen Kritis (Prioritas Tinggi)
Aktifkan komponen yang paling berdampak:

1. **Markdown Renderer** (`markdown_renderer.rs`)
   - Rendering pesan AI yang lebih baik
   - Code block highlighting
   
2. **Approval Bar** (`approval_bar.rs`)
   - HITL confirmation UI
   - Integrasi dengan policy engine

3. **Bash Block** (`bash_block.rs`)
   - Output command yang lebih readable
   - Syntax highlighting untuk shell

### Fase 3: Aktivasi Komponen Sekunder (Prioritas Sedang)
4. **File Diff** (`file_diff.rs`)
   - Preview perubahan file
   
5. **Side Panel** (`side_panel.rs`)
   - Context information
   - Task status

6. **Syntax Highlighter** (`syntax_highlighter.rs`)
   - Code highlighting

### Fase 4: Aktivasi Komponen Lanjutan (Prioritas Rendah)
7. **Plan Review** (`plan_review.rs`)
8. **Board Tasks** (`board_tasks.rs`)
9. **Custom Commands** (`custom_commands.rs`)
10. Komponen lainnya...

## Checklist Per Komponen

Untuk setiap file yang diaktifkan:

- [ ] Fix `use stakpak_*` imports → `use crate::tui::types::*`
- [ ] Update `AppState` references → `use crate::tui::app::types::AppState`
- [ ] Fix event handling → Map ke `InputEvent`/`OutputEvent`
- [ ] Test rendering
- [ ] Move dari `services_stakpak_disabled/` ke `services/`
- [ ] Export di `services.rs`
- [ ] Update `view.rs` untuk menggunakan komponen

## Estimasi Waktu

- **Fase 1**: 2-3 hari (persiapan)
- **Fase 2**: 3-5 hari (komponen kritis)
- **Fase 3**: 3-4 hari (komponen sekunder)
- **Fase 4**: 5-7 hari (komponen lanjutan)

**Total**: 13-19 hari kerja

## Risiko

1. **Breaking Changes**: Perubahan struktur data bisa break existing code
2. **Performance**: Komponen stakpak mungkin perlu optimasi
3. **Dependencies**: Mungkin perlu tambahan crate dependencies

## Next Steps

1. Mulai dengan audit dependencies (Fase 1)
2. Buat branch `feature/ui-migration`
3. Aktifkan markdown_renderer sebagai proof of concept
4. Iterasi untuk komponen lainnya
