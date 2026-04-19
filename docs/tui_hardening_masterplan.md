# VAC TUI Hardening Masterplan

**Status**: Active source of truth  
**Last updated**: 2026-04-18  
**Supersedes**: `IMPL_PLAN_V2.md`, `docs/ROADMAP_TO_100_v2.md`, legacy `.trae/specs/*` planning backlog

## Positioning

VAC saat ini harus diposisikan sebagai **advanced beta / pre-production**, bukan near-production. Fondasi arsitektur dan engine VIL sudah kuat, tetapi operator surface, startup truthfulness, command contract, telemetry surfacing, release trust chain, dan production evidence belum cukup rapi untuk diposisikan sejajar dengan Claude Code atau Stakpak sebagai alat operator-grade.

Target plan ini bukan menambah fitur TUI baru. Targetnya adalah mengubah VAC dari:

> feature-rich experimental cockpit

menjadi:

> truthful, deterministic, operator-grade control plane yang benar-benar memantulkan capability engine VIL

## Executive verdict

### Hal yang sudah kuat

- Arsitektur modular workspace dan pemisahan concern `vac_*` / `vil_*`
- VIL-native semantics, rulebook direction, dan swarm/control-plane foundation
- TUI sudah kaya surface, tetapi belum cukup disiplin
- Security posture mulai serius, tetapi closure operasional belum tuntas

### Hal yang masih menjadi blocker

- Boot screen belum truthful: `vunknown`, `Model: none`, hydration operator minim
- Command surface masih mencampur UI contract, agent passthrough, dan pseudo-command
- `controller.rs` terlalu monolitik dan rawan precedence/regression bug
- Telemetry engine penting belum naik ke UI
- Model/provider capability masih heuristic, belum registry contract nyata
- Approval dan shell governance masih terlalu UI-centric
- Input/output backpressure belum punya queue formal
- Production evidence dan real deployment audit masih pending

## Hard rules

1. Tidak ada fitur TUI baru sampai Wave 3 selesai.
2. Tidak ada command yang tampil ke operator jika action contract-nya belum jelas.
3. UI tidak boleh menyembunyikan state ambigu di balik fallback seperti `unknown` atau `none`.
4. Capability backend yang sudah ada harus disurfacing sebelum capability baru ditambahkan.
5. Trust chain dan operability tidak boleh dipasarkan sebagai selesai kalau gate fail-closed belum ada.

## Donor strategy vs Stakpak

### Yang didonor dari `stakpak/agent`

- routing modular untuk input/update/popup/shell/tool
- unified command registry dan executor
- popup request-on-open semantics
- pending outbound message queue dan flush-if-idle discipline
- shell approval parsing/policy separation
- session/reset/runtime lifecycle handling

### Yang tidak didonor

- VIL semantics
- rulebook meaning
- planner identity
- prompt behavior yang merusak arah VIL-native

Stakpak dipakai sebagai donor **infra/control-plane discipline**, bukan donor identitas produk.

## Current target score

- **Current honest position**: `advanced beta / pre-production`
- **Success condition untuk plan ini**: operator console menjadi truthful dan deterministic, lalu trust/release/evidence gates ditutup bertahap
- **Public claim policy**:
  - boleh klaim: advanced beta, VIL-native operator console, modular control-plane
  - tidak boleh klaim: near-production, production-ready competitor, operator-grade parity

## Workstream map

### Track A: Control-plane hardening

Ini adalah jalur utama untuk mengejar parity operator UX dengan Stakpak/Claude Code.

### Track B: Trust and release hardening

Ini adalah jalur wajib agar improvement UI tidak menutupi gap operasional:

- mutation gate fail-closed
- release trust chain benar-benar selesai
- disk quota / operability enforcement
- trace redaction end-to-end
- evidence gate 4 minggu

Track B boleh berjalan paralel, tetapi tidak boleh dibiarkan drift.

## Final roadmap

### Wave 0 — Freeze and debt register

**Goal**: hentikan penambahan surface baru di atas fondasi rapuh.

**Actions**

- feature freeze untuk TUI sampai Wave 3 selesai
- hapus semua backlog/spec plan lama dari source tree
- treat subtree `crates/vac_cli/src/tui/services_stakpak_disabled/**` sebagai debt register
- putuskan per file: `integrate`, `replace`, atau `delete`
- aktifkan status internal: `control-plane hardening active`

**Primary targets**

- `crates/vac_cli/src/tui/services_stakpak_disabled/**`
- `crates/vac_cli/src/tui/controller.rs`
- `docs/tui_hardening_masterplan.md`

**Acceptance gates**

- tidak ada feature TUI baru selama Wave 0-3
- setiap file di `services_stakpak_disabled` punya keputusan eksplisit
- semua plan lama sudah disupersede dan dihapus dari source tree

### Wave 1 — TUI kernel refactor

**Goal**: pecah giant controller menjadi handler graph yang bisa dipelihara.

**Donor pattern**

- `stakpak/agent:tui/src/services/update.rs`
- `stakpak/agent:tui/src/services/handlers/{input,popup,shell,tool,misc,navigation,text_selection}.rs`

**Implementation**

- tambah:
  - `crates/vac_cli/src/tui/update.rs`
  - `crates/vac_cli/src/tui/handlers/input.rs`
  - `crates/vac_cli/src/tui/handlers/popup.rs`
  - `crates/vac_cli/src/tui/handlers/approval.rs`
  - `crates/vac_cli/src/tui/handlers/review.rs`
  - `crates/vac_cli/src/tui/handlers/runtime.rs`
  - `crates/vac_cli/src/tui/handlers/navigation.rs`
  - `crates/vac_cli/src/tui/handlers/misc.rs`
  - `crates/vac_cli/src/tui/handlers/text_selection.rs`
- `controller.rs` menjadi façade tipis
- popup interception mengikuti guard yang setara dengan backend-event skip semantics

**Acceptance gates**

- `controller.rs` tidak lagi menjadi giant match untuk seluruh mode
- setiap popup/context punya handler sendiri
- ada regression test precedence input per context

### Wave 2 — Unified command system

**Goal**: hilangkan phantom commands dan samakan semua jalur eksekusi command.

**Implementation**

- tambah `crates/vac_cli/src/tui/services/commands.rs`
- definisikan:
  - `CommandId`
  - `CommandAction`
  - `CommandSurface`
  - `execute_command()`
- typed slash, helper dropdown, dan command palette harus memanggil executor yang sama
- reklasifikasi command lama:
  - built-in UI deterministik
  - built-in agent prompt
  - custom prompt
  - hidden/internal

**Explicit cleanup**

- `/vil`, `/swarm`, `/rulebook`, `/resume`, `/help` tidak boleh lagi tampil sebagai command operator-grade kalau hanya passthrough

**Acceptance gates**

- tidak ada command visible tanpa contract jelas
- typed slash == dropdown == palette behavior parity
- no phantom command tests

### Wave 3 — Startup hydration and truthful boot

**Goal**: first frame harus jujur, hidup, dan menjelaskan capability.

**Implementation**

- tambah `StartupSnapshot`
- hydrate sejak boot:
  - installed version
  - latest version jika ada
  - active/default model
  - profile dan rulebooks aktif
  - runtime environment
  - MCP server states
  - session count / checkpoint count
  - VIL profile detect result
- update:
  - `crates/vac_cli/src/tui/runner.rs`
  - `crates/vac_cli/src/tui/event_loop.rs`
  - `crates/vac_cli/src/tui/services/statusline.rs`
  - `crates/vac_cli/src/tui/services/helper_block.rs`
  - `crates/vac_cli/src/tui/view.rs`

**Truthfulness rules**

- jangan render `Model: none`
- render `no active model selected` bila memang belum aktif
- bedakan `configured default`, `active override`, dan `runtime provider`
- operator panels minimal menampilkan hydration status, bukan kosong tanpa konteks

**Acceptance gates**

- first frame tidak lagi `vunknown`
- first frame tidak lagi `Model: none`
- panel operator tidak lagi terlihat idle-kosong tanpa hydration status

### Wave 4 — Input/output reliability and backpressure

**Goal**: message send, paste, image attachment, dan busy state menjadi deterministic.

**Implementation**

- tambah `PendingUserMessage`
- tambah outbound queue `VecDeque`
- `flush_if_idle()`
- merge buffered messages
- retry-safe channel write semantics

**Targets**

- `crates/vac_cli/src/tui/app/types.rs`
- `crates/vac_cli/src/tui/handlers/input.rs`
- `crates/vac_cli/src/tui/runner.rs`

**Acceptance gates**

- no message loss saat loading/busy
- no duplicate send saat retry/cancel
- queue merge dan buffered flush punya e2e tests

### Wave 5 — Approval subsystem extraction

**Goal**: approvals bukan efek samping UI, tetapi state machine eksplisit.

**Implementation**

- tambah crate `crates/vac_approvals`
- domain types:
  - `ApprovalStateMachine`
  - `ApprovalDecision`
  - `ApprovalScope`
  - `ApprovalBatch`
  - `ShellCommandPolicyResolver`
  - `ToolApprovalPolicyResolver`

**UI responsibility**

- render batch
- select current
- approve/reject
- kirim decision

**Acceptance gates**

- approve current/all, reject current/all, reject with reason deterministic
- batch ordering deterministic
- no approval policy parsing tersisa di giant controller

### Wave 6 — Shell runtime hardening

**Goal**: shell mode menjadi subsystem nyata, bukan aksesoris input biasa.

**Implementation**

- tambah `crates/vac_cli/src/tui/services/shell_runtime.rs`
- refactor `handlers/shell.rs` menjadi UI-only
- jika perlu, pisahkan lifecycle shell ke crate/domain tersendiri
- explicit state diagram:
  - `Starting`
  - `Running`
  - `PromptReady`
  - `WaitingInput`
  - `Backgrounded`
  - `Exited`
  - `Killed`
  - `Errored`

**Acceptance gates**

- shell start/focus/background/kill deterministic
- no orphan shell session after session switch
- shell output bounded dan scroll behavior predictable

### Wave 7 — Runtime telemetry surfacing

**Goal**: UI benar-benar menunjukkan capability engine.

**Implementation**

- perluas mapping `RuntimeUpdate -> InputEvent` untuk:
  - validation score
  - validation issues summary
  - LSP diagnostics count
  - cancel reason
  - retry/backoff state
  - provider/model capability
  - task graph refresh
  - active tool timeline

**Targets**

- `crates/vac_cli/src/tui/runner.rs`
- `crates/vac_cli/src/tui/app/events.rs`
- `crates/vac_cli/src/tui/handlers/runtime.rs`
- `crates/vac_cli/src/tui/view.rs`

**Acceptance gates**

- operator bisa menjawab dari UI saja:
  - model aktif apa
  - validasi terakhir berapa
  - ada LSP issues atau tidak
  - tool aktif apa
  - task graph sedang di node mana

### Wave 8 — Model/profile/rulebook switcher hardening

**Goal**: switchers harus request-on-open, preselect current, searchable, dan deterministic.

**Implementation**

- setiap switcher punya lifecycle lengkap:
  - `open`
  - `update_filter`
  - `select_next`
  - `select_prev`
  - `submit_selected`
  - `close`
- request backend saat popup dibuka jika data bisa stale
- active item harus preselected
- popup menolak buka jika state lain sedang blocking

**Acceptance gates**

- `/model`, `/profile`, `/rulebook` usable keyboard-only
- no popup opens with stale empty state tanpa penjelasan

### Wave 9 — Changeset/review/editor unification

**Goal**: review, changeset popup, revert, dan editor integration menjadi satu domain.

**Implementation**

- tambah `services/changeset_domain.rs`
- pastikan semua surface baca dari single changeset store:
  - side panel changes
  - workbench review
  - popup file changes
  - open in editor
  - revert selected / filtered / all

**Acceptance gates**

- semua surface count/selection/status konsisten
- revert action terpantul ke semua surface

### Wave 10 — Workspace split for control-plane infra

**Goal**: discipline crate boundary mendekati Stakpak tanpa kehilangan identitas VIL.

**Recommended crates**

- `crates/vac_tui_runtime`
- `crates/vac_approvals`
- `crates/vac_shell`
- `crates/vac_changeset`
- `crates/vac_session_control`

**Acceptance gates**

- `vac_cli` tinggal entrypoint + adapter
- domain control-plane tidak lagi menumpuk di `vac_cli`
- VIL engine tetap hidup di `vac_core` / `vil_*`

### Wave 11 — Testing and evidence hardening

**Goal**: TUI integrity dibuktikan, bukan diklaim.

**Required test classes**

- command parity tests
- popup interception tests
- startup snapshot tests
- buffered message queue tests
- approval batch tests
- shell lifecycle tests
- session switch cleanup tests
- runtime telemetry propagation tests
- no phantom command tests
- no stale `unknown` / `none` startup tests

**Acceptance gates**

- `cargo test -p vac_cli --lib` menjadi behavioral safety net utama
- touched subsystems tidak diterima tanpa behavior tests

### Wave 12 — Product truthfulness polish

**Goal**: polish setelah arsitektur beres, bukan sebelumnya.

**Actions**

- banner/welcome rewrite
- empty-state rewrite
- activity taxonomy
- approval footer clarity
- command discoverability
- keyboard shortcut surfacing
- operator dashboard wording
- model/provider badges
- capability cards

**Acceptance gates**

- semua polish mengikuti contract state yang sudah benar
- tidak ada polish yang menyembunyikan ambiguity state

## Parallel mandatory hardening track

Track ini wajib jalan paralel dengan roadmap control-plane karena mempengaruhi trust claim produk.

### B1 — Mutation gate fail-closed

- hapus jalur `|| true` dan skip-success yang membuat mutation gate tidak tegas
- threshold harus benar-benar fail CI

### B2 — Release trust chain closure

- checksum contract di installer harus sesuai dengan artifact release aktual
- signing pipeline harus sinkron dengan dokumentasi
- public verification material harus tersedia

### B3 — Operability enforcement closure

- `memory_cap_bytes` dan `disk_quota_bytes` harus benar-benar enforced, bukan sekadar config surface

### B4 — Trace redaction end-to-end

- crash dump, trace recorder, export/import, tool arguments, dan runtime artifact harus tunduk ke satu redaction contract

### B5 — Evidence gate

- stability log 4 minggu
- release smoke evidence yang bisa diaudit
- re-audit delta yang bisa diverifikasi
- internal deployment entries hanya jika memang tersedia dari channel internal yang nyata

## Recommended implementation order

1. Wave 0
2. Wave 1
3. Wave 2
4. Wave 3
5. Wave 4
6. Wave 5
7. Wave 6
8. Wave 7
9. Wave 8
10. Wave 9
11. Wave 10
12. Wave 11
13. Wave 12

Parallel:

- B1-B4 mulai paling lambat saat Wave 4
- B5 hanya dibuka setelah control-plane stabil dan release chain jujur

## Completion definition

VAC baru boleh dinaikkan dari `advanced beta / pre-production` jika semua berikut benar:

- startup truthful
- command surface jujur dan unified
- engine telemetry penting tersurfacing
- approval dan shell governance sudah dipisah dari UI monolith
- trust/release/redaction gates fail-closed
- evidence stabilitas nyata sudah terkumpul

Sebelum itu, semua narasi publik harus tetap konservatif.
