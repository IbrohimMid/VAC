//! LRU cache for Kitty image DCS sequences (PR-T17).
//!
//! Encoding a PNG to a base64 DCS sequence is CPU-bound work that would
//! otherwise repeat on every render frame for the same image at the same
//! position. This cache stores the pre-computed byte sequences keyed on
//! `(png_content_hash, col, row)` and evicts the least-recently-used
//! entry when at capacity.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

/// Key for a cached DCS emission: image content + terminal cell position.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    png_hash: u64,
    col: u16,
    row: u16,
}

/// LRU cache mapping `(png_hash, col, row)` → pre-encoded DCS bytes.
pub struct KittyImageCache {
    capacity: usize,
    entries: HashMap<CacheKey, Vec<u8>>,
    order: VecDeque<CacheKey>,
}

impl KittyImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Return the cached DCS bytes for `(png, col, row)`, computing and
    /// caching them on first access. The returned slice is valid until the
    /// next mutable call on this cache.
    pub fn get_or_emit(&mut self, png: &[u8], col: u16, row: u16) -> &[u8] {
        let key = CacheKey {
            png_hash: hash_bytes(png),
            col,
            row,
        };

        if !self.entries.contains_key(&key) {
            // Evict the least-recently-used entry when at capacity.
            if self.entries.len() >= self.capacity {
                if let Some(lru_key) = self.order.pop_front() {
                    self.entries.remove(&lru_key);
                }
            }
            let dcs = super::emit_positioned_kitty_image(col, row, png);
            self.entries.insert(key.clone(), dcs);
            self.order.push_back(key.clone());
        } else {
            // Move to most-recently-used position.
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
        }

        self.entries.get(&key).map(Vec::as_slice).unwrap_or(b"")
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_A: &[u8] = b"fake-png-a";
    const PNG_B: &[u8] = b"fake-png-b";

    #[test]
    fn cache_returns_same_bytes_for_repeated_key() {
        let mut cache = KittyImageCache::new(4);
        let first = cache.get_or_emit(PNG_A, 5, 10).to_vec();
        let second = cache.get_or_emit(PNG_A, 5, 10).to_vec();
        assert!(!first.is_empty(), "DCS bytes must be non-empty for non-empty PNG");
        assert_eq!(first, second, "repeated call must return identical bytes");
    }

    #[test]
    fn cache_evicts_lru_on_capacity_full() {
        let mut cache = KittyImageCache::new(2);
        // Insert two entries to fill capacity.
        cache.get_or_emit(PNG_A, 0, 0);
        cache.get_or_emit(PNG_B, 0, 0);
        // Re-access PNG_A to make PNG_B the LRU entry.
        cache.get_or_emit(PNG_A, 0, 0);
        // Insert a third entry — PNG_B (LRU) should be evicted.
        cache.get_or_emit(PNG_A, 1, 0); // new key (col differs)
        assert_eq!(cache.entries.len(), 2, "cache must not exceed capacity");
        // PNG_A@(0,0) must still be present (recently used).
        let key_a = CacheKey { png_hash: hash_bytes(PNG_A), col: 0, row: 0 };
        assert!(cache.entries.contains_key(&key_a), "recently-used entry must survive eviction");
    }

    #[test]
    fn cache_different_position_stored_separately() {
        let mut cache = KittyImageCache::new(4);
        let bytes_at_00 = cache.get_or_emit(PNG_A, 0, 0).to_vec();
        let bytes_at_10 = cache.get_or_emit(PNG_A, 10, 0).to_vec();
        // The cursor escape differs between positions.
        assert_ne!(bytes_at_00, bytes_at_10, "different positions must produce different DCS bytes");
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn cache_different_png_stored_separately() {
        let mut cache = KittyImageCache::new(4);
        let bytes_a = cache.get_or_emit(PNG_A, 0, 0).to_vec();
        let bytes_b = cache.get_or_emit(PNG_B, 0, 0).to_vec();
        assert_ne!(bytes_a, bytes_b, "different PNG content must produce different DCS bytes");
        assert_eq!(cache.entries.len(), 2);
    }
}
