// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte-bounded LRU ownership for derived render payloads. Follows the host's
//! background_audio cache eviction contract, including oversized bypass.

use std::collections::HashMap;

pub(crate) struct ByteCache<T> {
    pub entries: HashMap<String, Entry<T>>,
    pub bytes: usize,
    cap: usize,
    serial: u64,
}

pub(crate) struct Entry<T> {
    pub value: T,
    bytes: usize,
    used: u64,
}

impl<T> ByteCache<T> {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            cap,
            serial: 0,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&T> {
        self.serial = self.serial.saturating_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.used = self.serial;
        Some(&entry.value)
    }

    pub fn insert(&mut self, key: String, value: T, bytes: usize) {
        self.remove(&key);
        if bytes > self.cap {
            return;
        }
        self.serial = self.serial.saturating_add(1);
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                used: self.serial,
            },
        );
        self.bytes += bytes;
        self.trim();
    }

    pub fn set_bytes(&mut self, key: &str, bytes: usize) {
        if bytes > self.cap {
            self.remove(key);
            return;
        }
        if let Some(entry) = self.entries.get_mut(key) {
            self.bytes -= entry.bytes;
            self.bytes += bytes;
            entry.bytes = bytes;
            self.trim();
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.bytes;
        }
    }

    fn trim(&mut self) {
        while self.bytes > self.cap || self.entries.len() > 4096 {
            let Some(key) = self
                .entries
                .iter()
                .min_by(|(a_key, a), (b_key, b)| (a.used, a_key).cmp(&(b.used, b_key)))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove(&key);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn byte_budget_evicts_lru_without_invalidating_a_live_owner() {
        let mut cache = ByteCache::new(12);
        let first = Arc::new(vec![1_u8; 8]);
        cache.insert("first".into(), Arc::clone(&first), 8);
        cache.insert("second".into(), Arc::new(vec![2; 4]), 4);
        cache.get("first");
        cache.insert("third".into(), Arc::new(vec![3; 4]), 4);
        assert!(cache.get("second").is_none());
        assert_eq!(cache.bytes, 12);
        cache.insert("huge".into(), Arc::new(vec![4; 13]), 13);
        assert!(cache.get("huge").is_none());
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert_eq!(first.as_slice(), &[1; 8]);
    }

    #[test]
    fn late_payload_accounting_evicts_oversized_inflight_slots() {
        let mut cache = ByteCache::new(12);
        cache.insert("warm".into(), (), 4);
        cache.insert("loading".into(), (), 0);
        cache.set_bytes("loading", 13);
        assert!(cache.get("loading").is_none());
        assert!(cache.get("warm").is_some());
        assert_eq!(cache.bytes, 4);
    }

    #[test]
    fn negative_cache_has_a_slot_bound_and_replacement_releases_its_old_charge() {
        let mut cache = ByteCache::new(12);
        cache.insert("same".into(), (), 8);
        cache.insert("same".into(), (), 4);
        assert_eq!(cache.bytes, 4);
        for i in 0..5000 {
            cache.insert(i.to_string(), (), 0);
        }
        assert_eq!(cache.entries.len(), 4096);
        assert_eq!(cache.bytes, 0);
    }
}
