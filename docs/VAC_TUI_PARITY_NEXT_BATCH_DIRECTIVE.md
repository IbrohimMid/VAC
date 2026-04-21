# VAC TUI Parity — Next Batch Directive (2026-04-21)

> Lanjutan dari `VAC_TUI_PARITY_REMAINING_PLAN.md` setelah audit commit `2a2e6ad` (PR-T12.1 ACCEPT) + `d6b572f` (PR-W25-9 ACCEPT WITH FIXUP).
> Dokumen ini = directive sempit untuk agent lokal berikutnya. Tiga PR, urutan ditentukan, DoD + commit message exact.

---

## Snapshot status per 2026-04-21 10:33 WIB

**Commit terakhir di `main`**: `d6b572f refactor(tui): split runner.rs + finish session_control async migration` (HEAD).

### Wave 2.5 — 7/9 landed

| PR | Status | Gap |
|---|---|---|
| W25-1 theme sweep | ❌ pending | 303 raw `Color::` baseline, ~140 di call-site non-palette |
| W25-2 TOML + notify | ✅ | — |
| W25-3 shortcuts_popup | ✅ | — |
| W25-4 input_popup | ✅ | — |
| W25-5 markdown_renderer | ✅ (borderline 575/557) | Optional further split Wave 4 |
| W25-6 view.rs | ✅ | — |
| W25-7 event_loop_tests | ⚠️ **partial** | `runtime.rs` = 804 LOC masih >600 |
| W25-8 app/types.rs | ✅ | — |
| W25-9 runner split + async | ⚠️ **partial** | `runner.rs` = 742 LOC masih >600 |

### Wave 3 — core 1/4 + prereq 2/4

- PR-T12 + T12.1 ✅ (bundled sebagai fitur)
- PR-T11, T13, T14 ❌
- §8 PR-2, PR-3 ✅ | §8 PR-5b, PR-6 ❌

### Exit-gate Wave 2.5 status

Files >600 LOC yang menghalangi exit-gate:
1. `crates/vac_tui_runtime/src/runner.rs` = 742
2. `crates/vac_tui_runtime/src/event_loop_tests/runtime.rs` = 804

Target exit-gate Wave 2.5: **0 file >600** di `crates/vac_tui_runtime/src/`. Dua PR di bawah menutup gate ini.

---

## Urutan eksekusi yang diputuskan

```
1. PR-W25-9.1  (runner.rs fixup)           ──┐
2. PR-W25-7.1  (event_loop_tests/runtime split)│ tutup exit-gate W2.5
                                               │
3. PR-W25-1    (theme sweep, incremental)    ──┘ batch terpisah
```

Alasan:
- #1 dan #2 zero overlap, independen, bisa paralel kalau ada bandwidth.
- #3 scope besar (~140 call site), harus incremental per-file; boleh dimulai kapan pun tanpa blok #1/#2.
- Setelah #1 + #2 landed, Wave 2.5 exit-gate **tertutup** → unlock Wave 3 PR berikutnya (PR-T11 dst).

---

## PR-W25-9.1 — Finish runner.rs split (742 → ≤420)

### Scope

Ekstrak sisa handler dari `runner.rs` ke submodul baru. Target `runner.rs` hanya berisi:
- `pub async fn run_vac_tui` (skeleton main loop)
- `fn classify_init_warning` (helper kecil, boleh tetap di root)
- `use` + re-exports

### Inventaris handler yang harus diekstrak

Dari grep struktur `runner.rs` (lines 109–497):

| Line range | Handler | Est. LOC | Target submodul |
|---|---|---|---|
| 109–181 | `OutputEvent::UserMessage` | ~72 | `runner/message_tasks.rs` (new) |
| 181–195 | `Accept/RejectTool` | ~14 | `runner/message_tasks.rs` |
| 195–250 | `SwitchToModel` + `SwitchProfile` | ~55 | `runner/profile_tasks.rs` (new) |
| 250–308 | `ApplyRulebooks` | ~58 | `runner/profile_tasks.rs` |
| 308–334 | `InvokeVilTool` | ~26 | `runner/vil_tasks.rs` (new) |
| 334–354 | `ExecuteCommand` (delegasi) | ~20 | sudah di `runner/shell_dispatch.rs` — pindahkan body kalau belum |
| 354–430 | `Export/ImportBundle` | ~76 | `runner/bundle_tasks.rs` (new) |
| 430–497 | sisanya (list/session routing, delegasi) | ~67 | sudah pakai submodul existing; sisa skeleton tetap |

### Files baru

- `crates/vac_tui_runtime/src/runner/message_tasks.rs` (~100 LOC)
- `crates/vac_tui_runtime/src/runner/profile_tasks.rs` (~120 LOC)
- `crates/vac_tui_runtime/src/runner/vil_tasks.rs` (~40 LOC)
- `crates/vac_tui_runtime/src/runner/bundle_tasks.rs` (~100 LOC)

### Strategi ekstraksi

Untuk setiap handler:
1. Ciptakan fungsi bebas `pub(super) async fn handle_<name>(state, deps..., payload) -> Result<()>` di submodul target.
2. Di `runner.rs`, ganti body match arm jadi satu-baris call: `runner::message_tasks::handle_user_message(&mut state, msg, tools, parts).await?;`
3. Import bersama di top `runner.rs`: `mod message_tasks; mod profile_tasks; ...`

### Definition of Done

- [ ] `wc -l crates/vac_tui_runtime/src/runner.rs` **≤ 420**
- [ ] Semua submodul baru **< 600 LOC**
- [ ] `grep -rn 'block_in_place' crates/vac_tui_runtime/src/ | grep -v '//' | wc -l` = 0 (tidak regresi)
- [ ] Tidak ada raw `Color::` baru
- [ ] Tidak ada `#[allow(dead_code|unused)]` baru
- [ ] `cargo nextest run -p vac_tui_runtime --lib` tidak menurunkan jumlah test vs baseline HEAD~1
- [ ] Tidak ada perubahan perilaku (pure mechanical extract — jangan ubah logic)

### Commit message (exact)

```
refactor(tui): PR-W25-9.1 extract runner.rs output handlers (742→≤420)
```

### Gotcha

- Borrow checker: `state: &mut AppState` sering dipakai bersama `sender.send(...)`. Kalau ekstraksi trigger E0499, return enum kecil dari handler ke caller untuk side-effect, jangan pass `sender` sekalian.
- `classify_init_warning` JANGAN dipindah — dipakai langsung dalam `run_vac_tui` skeleton.
- Visibility: pakai `pub(super)` bukan `pub` supaya surface module tidak bocor.

---

## PR-W25-7.1 — Split event_loop_tests/runtime.rs (804 → ≤420)

### Scope

`event_loop_tests/runtime.rs` (804 LOC, ~30KB) terlalu besar. Pecah per-feature sesuai konvensi yang sudah dipakai sibling (session/approval/shell/streaming).

### Rekomendasi struktur

```
event_loop_tests/
├── approval.rs        471 ✅
├── runtime.rs         ≤200 (harness + mod decl + re-export test helpers)
├── runtime/
│   ├── jobs.rs        (~250) cancel/retry/queue job tests
│   ├── snapshots.rs   (~220) state snapshot + restore tests
│   ├── agent_tasks.rs (~200) agent invocation + result handling tests
│   └── boot.rs        (~150) runtime initialization + teardown tests
├── session.rs         480 ✅
├── shell.rs           79  ✅
└── streaming.rs       4   (stub — optional consolidation atau buang kalau memang kosong)
```

### Strategi

1. `cat runtime.rs | grep -n '^    #\[tokio::test\]\|^    #\[test\]\|^    fn '` untuk daftar semua test + nama.
2. Klasifikasikan per kategori (`jobs`, `snapshots`, `agent_tasks`, `boot`).
3. Pindah test BERSAMA helper function lokalnya (kalau ada) — kalau helper dipakai lintas-kategori, promosikan ke `runtime.rs` (sekarang harness).
4. Jalankan count:
   ```
   # sebelum:
   cargo nextest run -p vac_tui_runtime --lib event_loop_tests::runtime 2>&1 | grep 'Starting\|PASS'
   # sesudah: jumlah test IDENTIK
   ```

### Definition of Done

- [ ] Semua file `event_loop_tests/runtime*.rs` **< 600 LOC**
- [ ] `runtime.rs` ≤ 200 LOC, fungsi hanya harness + `pub mod {jobs,snapshots,agent_tasks,boot};`
- [ ] Jumlah test `event_loop_tests::runtime::*` sebelum/sesudah **IDENTIK** (capture output sebelum mulai, paste di commit body)
- [ ] Pure mechanical move — 0 perubahan logic test
- [ ] Existing file sibling tidak tersentuh (approval/session/shell/streaming tetap)
- [ ] **BONUS**: kalau `streaming.rs` = 4 LOC cuma stub, hapus file + mod decl (jangan keep dead stub)

### Commit message (exact)

```
refactor(tui): PR-W25-7.1 split event_loop_tests/runtime — per-category submodules
```

### Gotcha

- `#[tokio::test]` perlu `tokio` scope → import konsisten dari `runtime.rs` atau duplicate `use tokio;` di tiap submodul. Konvensi pilih satu, dokumentasikan di header file.
- Fixture/helper yang dipakai banyak test → extract ke `runtime/helpers.rs` atau tetap di `runtime.rs` (harness). Jangan duplicate copy.

---

## PR-W25-1 — Theme sweep (incremental, multi-commit)

### Scope

303 raw `Color::` di `crates/vac_tui_runtime/src/{services,workbench}/` → migrasi ke `state.theme.style(StyleKey::*)`.

### Baseline distribusi (top file)

| File | Count | Catatan |
|---|---|---|
| `services/theme.rs` | 164 | **JANGAN SENTUH** — ini palette definition, `Color::Rgb(...)` memang benar |
| `services/detect_term.rs` | 37 | **REVIEW** — capability detection; mungkin legitimate. Audit per-case |
| `services/markdown_renderer/style.rs` | 16 | sweep |
| `services/text_selection.rs` | 12 | sweep |
| `services/side_panel.rs` | 11 | sweep |
| `workbench/runtime.rs` | 8 | sweep |
| `services/vil_workbench/render.rs` | 7 | sweep |
| `services/helper_dropdown.rs` | 7 | sweep |
| `workbench/sessions.rs` | 6 | sweep |
| `services/message.rs` | 6 | sweep |
| … sisa ~30 file | ~29 | sweep per-file |

**Real sweep target**: 303 − 164 (theme.rs) − (detect_term legit count) ≈ **~100 call site** di ~25 file.

### Strategi incremental

**Satu commit per file**, urut dari kecil ke besar (momentum + review mudah):

```bash
# Tahap 1 — leaf files (≤10 sites each):
for FILE in $(grep -rln 'Color::' crates/vac_tui_runtime/src/services crates/vac_tui_runtime/src/workbench \
              | grep -v theme.rs | grep -v detect_term.rs); do
  echo "$FILE: $(grep -c 'Color::' $FILE | grep -v '//')"
done | sort -k2 -n -t:
```

Per file:
1. `grep -n 'Color::' <file>` → list setiap pemanggilan.
2. Untuk setiap `Color::<X>`:
   - Kalau ada `StyleKey` yang sudah cocok → pakai.
   - Kalau belum ada → **tambah** key baru di `services/theme.rs` enum + 3 palette (Dark, Light, HighContrast) + daftar `all_presets_cover_all_keys` test.
3. Replace di call site: `Color::Red` → `state.theme.style(StyleKey::ValidationError).fg.unwrap()` (atau pakai `.style()` langsung kalau konteksnya `Style`, bukan `Color`).
4. `grep 'Color::' <file>` harus return 0 setelah commit.

### Definition of Done (per-file commit)

- [ ] `grep -c 'Color::' <file> | grep -v '//'` = 0
- [ ] StyleKey baru (kalau ditambah) hadir di enum + 3 palette + test list
- [ ] Semantic sama (visual test manual kalau memungkinkan, minimal build bersih)
- [ ] `cargo nextest run -p vac_tui_runtime --lib services::theme` hijau (test `all_presets_cover_all_keys`)

### Definition of Done (exit-gate PR-W25-1)

- [ ] `grep -rn 'Color::' crates/vac_tui_runtime/src/services crates/vac_tui_runtime/src/workbench | grep -v '//' | grep -v theme.rs | grep -v detect_term.rs | wc -l` = 0
- [ ] `services/theme.rs` tetap berisi palette definitions (tidak tersentuh kecuali penambahan StyleKey)
- [ ] `services/detect_term.rs` direview per-case: kalau legitimate (low-level terminal capability), biarkan dengan komentar `// legitimate: terminal capability byte`; kalau user-facing, sweep juga.

### Commit message per-file (exact template)

```
refactor(tui): migrate <relative-path-of-file> to theme StyleKey
```

Contoh: `refactor(tui): migrate services/side_panel.rs to theme StyleKey`

### Commit message final (opsional summary)

```
chore(tui): PR-W25-1 complete theme sweep — 0 raw Color:: in services/workbench
```

### Gotcha

- `Color::Reset` → pakai `Style::default()` atau `StyleKey::Neutral` (tambah kalau belum ada).
- `Color::Rgb(r, g, b)` brand color → **buat StyleKey dengan nama semantik** (mis. `BrandAccent`, `LogoPrimary`), bukan keep raw.
- Beberapa site mungkin pakai `Color` dalam context `ratatui::style::Color` (bukan `Style`). Kalau accessor `state.theme.style(...)` return `Style`, extract `.fg` atau bikin helper `state.theme.color(StyleKey::*) -> Color`.
- Kalau ragu untuk 1 site (mis. gradient calc), tandai dengan `// FIXME(W25-1): dynamic color, skip` + issue/TODO list. Jangan force sweep yang merusak semantic.

---

## Audit rubric untuk ketiga PR

Sama persis dengan §13 di `VAC_TUI_PARITY_REMAINING_PLAN.md`. Ringkasan:

- (a) **Code reviewer** — commit message exact, scope bersih, no new `Color::`, no `unwrap()` production, no `#[allow]` baru.
- (b) **Architect** — file <600, layering bersih, no sync I/O regression (`scripts/check_sync_io.sh` lulus).
- (c) **Engineer** — test names sesuai spec hadir & hijau, count tests tidak menurun (kritikal untuk W25-7.1).
- (d) **Operasional** — tidak regresi 2 pre-existing failing test (privacy AWS ARN, scheduler monitor_mode); tidak ada artifact stale.

### Format submission untuk audit

```
## PR-W25-X.Y Report

- Commit: <hash> <message>
- Files: <git show --stat summary>
- Tests: <cargo nextest run output tail ~20 baris, termasuk PASS/FAIL + total count>
- Exit gate check:
  - runner.rs: <LOC>
  - event_loop_tests/runtime.rs: <LOC>
  - Color::: <grep count>
- Deviasi dari plan (kalau ada + justifikasi): <...>
```

---

## Dashboard update wajib per PR

Setiap PR selesai, update `docs/VAC_TUI_PARITY_DASHBOARD.md`:

- Commit sha baru (kolom paling kanan timeline).
- File >600 count (W25-9.1 turunkan 1, W25-7.1 turunkan 1, W25-1 tidak ubah).
- Raw `Color::` count (W25-1 incremental — update tiap batch).
- Tests total (dari `cargo nextest run` summary).
- Wave 2.5 progress (W25-9.1 dan W25-7.1 tidak menambah PR count karena ini fixup dari W25-9 dan W25-7, tapi tandai dengan note `W25-9 closed (via W25-9.1)` di kolom Notes).

Bundle dashboard update di commit PR yang sama (jangan commit terpisah kecuali untuk W25-1 yang multi-commit — boleh 1 dashboard update di akhir batch).

---

## Kalau salah satu PR gagal

- **Borrow checker / compile fail** di W25-9.1 → stop, paste error ke thread ini, saya bantu redesign signature.
- **Test count turun** di W25-7.1 → stop, verifikasi test mana yang tidak ikut pindah.
- **Visual regression** di W25-1 → stop sebelum commit file itu, paste screenshot/context, diskusi mapping StyleKey.
- **MCP / network flakiness** → jangan retry lebih dari 3 kali; laporkan ke thread.

---

## Prioritas setelah batch ini

Begitu W25-9.1, W25-7.1, dan W25-1 semua landed, exit-gate Wave 2.5 resmi **CLOSED**. Urutan Wave 3 berikutnya:

1. **§8 PR-5b** `vwfd_parity_pass` (fondasi diff)
2. **§8 PR-6** VWFD semantic diff
3. **PR-T11** VWFD inspector
4. **PR-T13** VIL-aware diff overlay (butuh PR-5b + PR-6)
5. **PR-T14** Background vil dev

Spec lengkap di `VAC_TUI_PARITY_REMAINING_PLAN.md` §7–11.
