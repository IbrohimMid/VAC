- [x] Approval routing ACP ter-scope ke task/session (bukan active handle global tanpa verifikasi)
- [x] Approval untuk stale/wrong-target menghasilkan error jelas dan tidak memutasi approval record
- [x] `reject_tool` simetris end-to-end terhadap approve (tool tidak dieksekusi, outcome observable)
- [x] Cleanup registry approval terjadi saat task selesai (channel closed) untuk mencegah stale routing
- [x] Ada ACP protocol e2e/integration test: approve, reject, stale/wrong-target, overlap
- [x] `cargo test -p vac_core -p vac_cli` lulus

