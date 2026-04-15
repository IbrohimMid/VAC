use crate::error::{ContextError, ContextResult};
use memmap2::MmapMut;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Serialize, Deserialize)]
pub struct Allocation {
    pub offset: usize,
    pub size: usize,
}

impl std::fmt::Debug for Allocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Allocation")
            .field("offset", &self.offset)
            .field("size", &self.size)
            .finish()
    }
}

pub struct ShmArena {
    mmap: Arc<RwLock<MmapMut>>,
    size: usize,
    path: std::path::PathBuf,
    free_list: Arc<RwLock<Vec<(usize, usize)>>>,
    allocations: Arc<RwLock<HashMap<usize, Allocation>>>,
    next_id: Arc<RwLock<usize>>,
}

impl std::fmt::Debug for ShmArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmArena")
            .field("size", &self.size)
            .field("path", &self.path)
            .finish()
    }
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
            free_list: Arc::new(RwLock::new(vec![(0, size)])),
            allocations: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(0)),
        })
    }

    pub async fn allocate(&self, size: usize) -> ContextResult<Allocation> {
        let mut free_list = self.free_list.write().await;

        if let Some(pos) = free_list
            .iter()
            .position(|&(_, free_size)| free_size >= size)
        {
            let (offset, _) = free_list[pos];
            let allocation = Allocation { offset, size };

            free_list[pos] = (offset + size, free_list[pos].1 - size);
            if free_list[pos].1 == 0 {
                free_list.remove(pos);
            }

            let mut allocs = self.allocations.write().await;
            let mut next_id = self.next_id.write().await;
            let id = *next_id;
            *next_id += 1;
            allocs.insert(id, allocation.clone());

            Ok(allocation)
        } else {
            Err(ContextError::ShmFull)
        }
    }

    pub async fn allocate_and_write(&self, data: &[u8]) -> ContextResult<Allocation> {
        let allocation = self.allocate(data.len()).await?;

        let mut mmap = self.mmap.write().await;
        mmap[allocation.offset..allocation.offset + data.len()].copy_from_slice(data);

        Ok(allocation)
    }

    pub async fn free(&self, offset: usize) -> ContextResult<()> {
        let mut allocs = self.allocations.write().await;
        let mut free_list = self.free_list.write().await;

        let allocation = allocs
            .iter()
            .find(|(_, a)| a.offset == offset)
            .map(|(_, a)| a.clone())
            .ok_or_else(|| ContextError::OutOfBounds("Invalid allocation offset".to_string()))?;

        allocs.retain(|_, a| a.offset != offset);

        let mut inserted = false;
        for (i, (free_offset, _)) in free_list.iter_mut().enumerate() {
            if *free_offset > allocation.offset {
                free_list.insert(i, (allocation.offset, allocation.size));
                inserted = true;
                break;
            }
        }
        if !inserted {
            free_list.push((allocation.offset, allocation.size));
        }

        free_list.sort_by_key(|(o, _)| *o);

        self.coalesce_free_list().await;

        Ok(())
    }

    async fn coalesce_free_list(&self) {
        let mut free_list = self.free_list.write().await;
        if free_list.len() < 2 {
            return;
        }

        let mut i = 0;
        while i < free_list.len() - 1 {
            let (off1, size1) = free_list[i];
            let (off2, size2) = free_list[i + 1];

            if off1 + size1 == off2 {
                free_list[i] = (off1, size1 + size2);
                free_list.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    pub async fn write(&self, offset: usize, data: &[u8]) -> ContextResult<usize> {
        if offset + data.len() > self.size {
            return Err(ContextError::OutOfBounds(
                "Write would exceed arena bounds".to_string(),
            ));
        }

        let mut mmap = self.mmap.write().await;
        mmap[offset..offset + data.len()].copy_from_slice(data);
        Ok(data.len())
    }

    pub async fn read(&self, offset: usize, len: usize) -> ContextResult<Vec<u8>> {
        if offset + len > self.size {
            return Err(ContextError::OutOfBounds(
                "Read would exceed arena bounds".to_string(),
            ));
        }

        let mmap = self.mmap.read().await;
        Ok(mmap[offset..offset + len].to_vec())
    }

    pub async fn write_string(&self, offset: usize, data: &str) -> ContextResult<usize> {
        self.write(offset, data.as_bytes()).await
    }

    pub async fn read_string(&self, offset: usize, len: usize) -> ContextResult<String> {
        let bytes = self.read(offset, len).await?;
        String::from_utf8(bytes).map_err(|e| ContextError::Retrieval(e.to_string()))
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.mmap.blocking_read().as_ptr()
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub async fn bytes_allocated(&self) -> usize {
        let allocs = self.allocations.read().await;
        allocs.values().map(|a| a.size).sum()
    }

    pub async fn bytes_free(&self) -> usize {
        let free_list = self.free_list.read().await;
        free_list.iter().map(|(_, s)| s).sum()
    }
}
