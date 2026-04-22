use vac_signal::{Distiller, RegexScorer, SignalBuffer, SignalStreamKind, TailDistiller};

#[test]
fn end_to_end_buffer_score_distill() {
    let mut buf = SignalBuffer::new(SignalStreamKind::Shell, 64);
    buf.push_chunk("starting\nWARN: slow\nError: boom\nok\n");

    let scorer = RegexScorer::default_heuristics();
    let distiller = TailDistiller::new(&scorer, 2);
    let view = distiller.distill(&buf);

    assert!(view.key_lines.iter().any(|l| l.contains("Error: boom")));
    assert_eq!(view.tail, vec!["Error: boom", "ok"]);
}
