use std::fs::File;
use std::io::{Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub fn write_index(paths: &[PathBuf], out_path: &Path) -> std::io::Result<()> {
    let mut w = BufWriter::new(File::create(out_path)?);
    w.write_all(&[1])?; // version
    
    // Dedup and tokenize
    let mut seen = std::collections::HashSet::new();
    let mut docs = Vec::new();
    for p in paths {
        if seen.insert(p.as_path()) {
            let tokens = tokenize_path(p);
            docs.push((p, tokens));
        }
    }
    
    w.write_all(&(docs.len() as u32).to_le_bytes())?;
    for (p, tokens) in docs {
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

pub fn read_index(in_path: &Path) -> std::io::Result<Vec<(PathBuf, Vec<String>)>> {
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
    Ok(docs)
}
