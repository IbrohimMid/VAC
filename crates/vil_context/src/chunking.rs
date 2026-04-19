pub struct SemanticChunker {
    chunk_size: usize,
    chunk_overlap: usize,
}

impl SemanticChunker {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        debug_assert!(chunk_size > 0);
        debug_assert!(chunk_overlap < chunk_size);
        Self {
            chunk_size,
            chunk_overlap,
        }
    }

    pub fn chunk(&self, text: &str) -> Result<Vec<String>, String> {
        let words: Vec<&str> = text.split_whitespace().collect();

        if words.is_empty() {
            return Ok(vec![]);
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < words.len() {
            let mut window = Vec::new();
            let mut count = 0;

            for word in words.iter().skip(start) {
                let word_len = word.len();
                let added_len = if count == 0 { word_len } else { word_len + 1 };

                if count + added_len > self.chunk_size && count > 0 {
                    break;
                }

                window.push(*word);
                count += added_len;
            }

            let chunk = window.join(" ");
            if !chunk.is_empty() {
                chunks.push(chunk);
            }

            let step = std::cmp::max(1, window.len().saturating_sub(self.chunk_overlap));
            start += step;
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_overlap_boundary_does_not_infinite_loop() {
        // chunk_size = 3 bytes, overlap = 2 words
        // When words are "a", "b", "c", "d", "e"
        // Window 1: "a b" (len=3 bytes, 2 words)
        // With chunk_overlap = 2, step would be 2 - 2 = 0 without max(1) -> infinite loop
        let chunker = SemanticChunker::new(3, 2);
        let text = "a b c d e";
        let chunks = chunker.chunk(text).unwrap();
        assert_eq!(chunks, vec!["a b", "b c", "c d", "d e", "e"]);
    }
}
