#!/bin/bash
# Script untuk menganalisis dependencies UI stakpak yang perlu dimigrasi

DISABLED_DIR="crates/vac_cli/src/tui/services_stakpak_disabled"

echo "=== Analisis Dependencies UI Stakpak ==="
echo ""

echo "1. Files dengan 'use stakpak' imports:"
echo "----------------------------------------"
grep -r "use stakpak" "$DISABLED_DIR" --include="*.rs" | cut -d: -f1 | sort -u | wc -l
echo "files ditemukan"
echo ""

echo "2. Files dengan 'AppState' references:"
echo "----------------------------------------"
grep -r "AppState" "$DISABLED_DIR" --include="*.rs" | cut -d: -f1 | sort -u | wc -l
echo "files ditemukan"
echo ""

echo "3. Top 10 imports yang paling sering digunakan:"
echo "------------------------------------------------"
grep -rh "^use " "$DISABLED_DIR" --include="*.rs" | \
    grep -v "^use crate" | \
    grep -v "^use std" | \
    sort | uniq -c | sort -rn | head -10
echo ""

echo "4. External crate dependencies:"
echo "--------------------------------"
grep -rh "^use " "$DISABLED_DIR" --include="*.rs" | \
    grep -v "^use crate" | \
    grep -v "^use std" | \
    grep -v "^use super" | \
    cut -d: -f1 | \
    awk '{print $2}' | \
    cut -d: -f1 | \
    sort -u
echo ""

echo "5. Ukuran komponen (lines of code):"
echo "------------------------------------"
find "$DISABLED_DIR" -name "*.rs" -type f -exec wc -l {} + | sort -rn | head -15
echo ""

echo "=== Rekomendasi Prioritas Migrasi ==="
echo ""
echo "Berdasarkan kompleksitas dan impact:"
echo "1. markdown_renderer.rs (76KB) - High impact"
echo "2. approval_bar.rs (25KB) - Critical untuk HITL"
echo "3. bash_block.rs (155KB) - Command output"
echo "4. file_diff.rs (25KB) - File preview"
echo "5. side_panel.rs (36KB) - Context info"
