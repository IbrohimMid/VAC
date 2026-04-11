//! Streaming response utilities.

use crate::provider::StreamChunk;
use tokio::sync::mpsc;

pub async fn collect_text(mut rx: mpsc::Receiver<StreamChunk>) -> String {
    let mut text = String::new();
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(t) => text.push_str(&t),
            StreamChunk::Done(_) => break,
            StreamChunk::Error(e) => {
                tracing::error!(error = %e, "Stream error");
                break;
            }
            _ => {}
        }
    }
    text
}

pub async fn print_stream(mut rx: mpsc::Receiver<StreamChunk>) {
    use std::io::Write;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Text(t) => {
                print!("{}", t);
                let _ = std::io::stdout().flush();
            }
            StreamChunk::Done(usage) => {
                println!("\n[Done: {} tokens]", usage.total_tokens);
                break;
            }
            StreamChunk::Error(e) => {
                eprintln!("\n[Error: {}]", e);
                break;
            }
            _ => {}
        }
    }
}
