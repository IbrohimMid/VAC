# Tasks

- [x] Task 1: Wave 0 - TUI Integrity Sweep
  - [x] SubTask 1.1: Audit slash command di `helper_block.rs` dan klasifikasikan.
  - [x] SubTask 1.2: Deduplikasi key binding di `event.rs`.
  - [x] SubTask 1.3: Audit `InputEvent` di `events.rs` dan pastikan ada handlernya.
  - [x] SubTask 1.4: Audit footer/help/banner hints di `view.rs`.
  - [x] SubTask 1.5: Tambahkan test contract untuk command dispatch dan keymap uniqueness.

- [x] Task 2: Wave 1 - TUI Kernel Unification
  - [x] SubTask 2.1: Pecah reducer besar di `event_loop.rs` menjadi surface controller.
  - [x] SubTask 2.2: Tambahkan `ActionRegistry` (`tui/action_registry.rs`).
  - [x] SubTask 2.3: Standardisasi overlay contract.
  - [x] SubTask 2.4: Bangun statusline permanen.
  - [x] SubTask 2.5: Hilangkan routing cabang yang duplikatif.

- [x] Task 3: Wave 2 - Repo-Native Coding Workbench
  - [x] SubTask 3.1: Implementasi Git Workbench (status tree, commit composer, dll).
  - [x] SubTask 3.2: Implementasi Patch/Hunk review.
  - [x] SubTask 3.3: Implementasi Multi-shell manager.
  - [x] SubTask 3.4: Implementasi Repo navigator baseline.

- [x] Task 4: Wave 3 - Repo Intelligence & Context Composer
  - [x] SubTask 4.1: Bangun `RepoIndexService`.
  - [x] SubTask 4.2: Implementasi Unified search.
  - [x] SubTask 4.3: Implementasi Context composer.
  - [x] SubTask 4.4: Implementasi Model capability matrix.

- [x] Task 5: Wave 4 - Autonomous Task Graph
  - [x] SubTask 5.1: Tambahkan `TaskGraph` eksplisit.
  - [x] SubTask 5.2: Bangun Inspector UI untuk task graph.
  - [x] SubTask 5.3: Implementasi optional worktree per subtask.
  - [x] SubTask 5.4: Implementasi Approval policy profiles.

- [x] Task 6: Wave 5 - VIL Operator Superpowers
  - [x] SubTask 6.1: Implementasi structured issue schema.
  - [x] SubTask 6.2: Bangun VIL campaign mode.
  - [x] SubTask 6.3: Implementasi Validation heatmap.
  - [x] SubTask 6.4: Bangun Rulebook cockpit.
  - [x] SubTask 6.5: Implementasi Semantic repair loop.

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 2]
- [Task 4] depends on [Task 2]
- [Task 5] depends on [Task 3]
- [Task 6] depends on [Task 2]
