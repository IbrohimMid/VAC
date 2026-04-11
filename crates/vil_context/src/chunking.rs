use std::collections::VecDeque;

pub struct SemanticChunker {
    chunk_size: usize,
    chunk_overlap: usize,
}

impl SemanticChunker {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
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
        let mut window: VecDeque<&str> = VecDeque::new();
        let mut start = 0;

        while start < words.len() {
            window.clear();

            let mut count = 0;
            for word in words.iter().skip(start) {
                let word_len = word.len();
                if count + word_len > self.chunk_size && count > 0 {
                    break;
                }
                window.push_back(word);
                count += word_len + 1;
            }

            let chunk: String = window.iter().copied().collect::<Vec<_>>().join(" ");
            if !chunk.is_empty() {
                chunks.push(chunk);
            }

            if window.len() < self.chunk_overlap {
                break;
            }

            start += window.len().saturating_sub(self.chunk_overlap);
        }

        Ok(chunks)
    }
}
