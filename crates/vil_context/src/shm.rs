use crate::error::ContextResult;
use memmap2::MmapMut;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ShmArena {
    mmap: Arc<RwLock<MmapMut>>,
    size: usize,
    #[allow(dead_code)]
    path: std::path::PathBuf,
}

impl ShmArena {
    pub fn new(path: &Path, size: usize) -> Result<Self, std::io::Error> {
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(f) => f,
            Err(_) => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?,
        };

        file.set_len(size as u64)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            mmap: Arc::new(RwLock::new(mmap)),
            size,
            path: path.to_path_buf(),
        })
    }

    pub async fn write(&self, offset: usize, data: &[u8]) -> ContextResult<usize> {
        if offset + data.len() > self.size {
            return Err(crate::error::ContextError::ShmAllocation(
                "Write would exceed arena bounds".to_string(),
            ));
        }

        let mut mmap = self.mmap.write().await;
        mmap[offset..offset + data.len()].copy_from_slice(data);
        Ok(data.len())
    }

    pub async fn read(&self, offset: usize, len: usize) -> ContextResult<Vec<u8>> {
        if offset + len > self.size {
            return Err(crate::error::ContextError::ShmAllocation(
                "Read would exceed arena bounds".to_string(),
            ));
        }

        let mmap = self.mmap.read().await;
        Ok(mmap[offset..offset + len].to_vec())
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.mmap.blocking_read().as_ptr()
    }

    pub fn size(&self) -> usize {
        self.size
    }
}
