# Checklist

- [x] Wave 0: Tidak ada feature TUI baru; semua file di `services_stakpak_disabled` punya keputusan eksplisit; plan lama dihapus
- [x] Wave 1: `controller.rs` menjadi façade tipis; setiap context punya handler sendiri dengan regression test
- [x] Wave 2: Tidak ada command visible tanpa contract jelas; typed slash == dropdown == palette behavior parity
- [x] Wave 3: First frame tidak lagi `vunknown` atau `Model: none`; hydration status terlihat jelas
- [x] Wave 4: Tidak ada message loss/duplicate send; queue merge dan buffered flush memiliki e2e tests
- [x] Wave 5: Approve/reject deterministic; batch ordering deterministic; tidak ada approval policy parsing di giant controller
- [x] Wave 6: Shell lifecycle deterministic; tidak ada orphan shell session; output bounded dan scroll predictable
- [x] Wave 7: UI dapat menampilkan model aktif, validasi terakhir, LSP issues, tool aktif, task graph secara real-time
- [x] Wave 8: Switcher usable keyboard-only; tidak ada popup membuka state stale kosong tanpa penjelasan
- [x] Wave 9: Semua surface count/selection/status konsisten; revert action terpantul ke semua surface
- [x] Wave 10: Domain control-plane tidak menumpuk di `vac_cli`; VIL engine tetap di `vac_core`/`vil_*`
- [x] Wave 11: Semua touched subsystems lulus behavior tests dan `cargo test -p vac_cli --lib` menjadi safety net utama
- [x] Wave 12: Polish selesai dan mengikuti contract state yang benar tanpa menyembunyikan ambiguity
- [x] Track B1-B5: Mutation gate fail-closed, release trust chain sinkron, memory/disk quota enforced, trace redaction terpusat, evidence stabilitas ada
