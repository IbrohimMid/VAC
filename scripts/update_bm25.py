import re

with open('/workspace/crates/vac_ingest/src/bm25.rs', 'r') as f:
    content = f.read()

new_code = """
use std::fs::File;
use std::io::{Read, Write, BufReader, BufWriter};

#[derive(Debug, Clone)]
pub struct Bm25Index {
    pub docs: Vec<(PathBuf, Vec<String>)>,
}

impl Bm25Index {
    pub fn build(paths: &[PathBuf]) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut docs = Vec::new();
        for p in paths {
            if seen.insert(p.as_path()) {
                let tokens = tokenize_path(p);
                docs.push((p.clone(), tokens));
            }
        }
        Self { docs }
    }

    pub fn write_to_file(&self, out_path: &Path) -> std::io::Result<()> {
        let mut w = BufWriter::new(File::create(out_path)?);
        w.write_all(&[1])?; // version
        w.write_all(&(self.docs.len() as u32).to_le_bytes())?;
        for (p, tokens) in &self.docs {
            let p_str = p.to_string_lossy();
            let p_bytes = p_str.as_bytes();
            w.write_all(&(p_bytes.len() as u16).to_le_bytes())?;
            w.write_all(p_bytes)?;
            
            w.write_all(&(tokens.len() as u16).to_le_bytes())?;
            for t in tokens {
                let t_bytes = t.as_bytes();
                w.write_all(&(t_bytes.len() as u8).to_le_bytes())?;
                w.write_all(t_bytes)?;
            }
        }
        w.flush()
    }

    pub fn read_from_file(in_path: &Path) -> std::io::Result<Self> {
        let mut r = BufReader::new(File::open(in_path)?);
        let mut ver = [0u8; 1];
        r.read_exact(&mut ver)?;
        if ver[0] != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported version"));
        }
        
        let mut num_buf = [0u8; 4];
        r.read_exact(&mut num_buf)?;
        let num_docs = u32::from_le_bytes(num_buf);
        
        let mut docs = Vec::with_capacity(num_docs as usize);
        for _ in 0..num_docs {
            let mut len_buf = [0u8; 2];
            r.read_exact(&mut len_buf)?;
            let p_len = u16::from_le_bytes(len_buf) as usize;
            let mut p_bytes = vec![0u8; p_len];
            r.read_exact(&mut p_bytes)?;
            let path = PathBuf::from(String::from_utf8_lossy(&p_bytes).into_owned());
            
            r.read_exact(&mut len_buf)?;
            let num_tokens = u16::from_le_bytes(len_buf) as usize;
            let mut tokens = Vec::with_capacity(num_tokens);
            for _ in 0..num_tokens {
                let mut t_len_buf = [0u8; 1];
                r.read_exact(&mut t_len_buf)?;
                let t_len = t_len_buf[0] as usize;
                let mut t_bytes = vec![0u8; t_len];
                r.read_exact(&mut t_bytes)?;
                tokens.push(String::from_utf8_lossy(&t_bytes).into_owned());
            }
            docs.push((path, tokens));
        }
        Ok(Self { docs })
    }

    pub fn rank(&self, query: &str, top_k: usize, params: Bm25Params) -> Vec<RankedPath> {
        if self.docs.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let q_tokens = tokenize(query);
        if q_tokens.is_empty() {
            return Vec::new();
        }

        let n = self.docs.len() as f32;
        let avgdl = if self.docs.is_empty() {
            1.0
        } else {
            self.docs.iter().map(|(_, d)| d.len()).sum::<usize>() as f32 / self.docs.len() as f32
        };

        // Document frequencies.
        let mut df: HashMap<&str, usize> = HashMap::new();
        for (_, doc) in &self.docs {
            let unique: std::collections::HashSet<&str> =
                doc.iter().map(|s| s.as_str()).collect();
            for t in unique {
                *df.entry(t).or_insert(0) += 1;
            }
        }

        let mut scored: Vec<RankedPath> = self.docs
            .iter()
            .map(|(p, doc)| {
                let dl = doc.len() as f32;
                let mut score = 0.0_f32;
                for q in &q_tokens {
                    let f = doc.iter().filter(|t| *t == q).count() as f32;
                    if f == 0.0 {
                        continue;
                    }
                    let dfv = df.get(q.as_str()).copied().unwrap_or(0) as f32;
                    let idf = ((n - dfv + 0.5) / (dfv + 0.5) + 1.0).ln();
                    let denom = f + params.k1 * (1.0 - params.b + params.b * (dl / avgdl));
                    let numer = f * (params.k1 + 1.0);
                    score += idf * (numer / denom);
                }
                RankedPath {
                    path: p.clone(),
                    score,
                }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }
}

pub fn rank_paths(
    paths: &[PathBuf],
    query: &str,
    top_k: usize,
    params: Bm25Params,
) -> Vec<RankedPath> {
    let index = Bm25Index::build(paths);
    index.rank(query, top_k, params)
}
"""

content = re.sub(r'pub fn rank_paths\([\s\S]*?scored\n\}', new_code.strip(), content)

with open('/workspace/crates/vac_ingest/src/bm25.rs', 'w') as f:
    f.write(content)

