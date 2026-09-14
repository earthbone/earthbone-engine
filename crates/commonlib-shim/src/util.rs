// SPDX-License-Identifier: MIT

//! Containers, strings and locks. Rust std stands in for every one of
//! them, so this module is the mapping, not an implementation.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// `RE::BSFixedString`: an interned, case-insensitive string. Two fixed
/// strings that differ only by case are the same string in the game.
#[derive(Clone, Debug)]
pub struct FixedString(Arc<str>);

impl FixedString {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for FixedString {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
}
impl Eq for FixedString {}

/// The intern table behind [`FixedString`].
#[derive(Default)]
pub struct StringCache {
    table: RwLock<HashMap<String, Arc<str>>>,
}

impl StringCache {
    pub fn intern(&self, s: &str) -> FixedString {
        let key = s.to_lowercase();
        // CONTEXT: a poisoned lock still holds a valid map, a panicking reader
        // never partially wrote it, so recovering the guard is safe here.
        if let Some(a) = self
            .table
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return FixedString(a.clone());
        }
        let a: Arc<str> = Arc::from(s);
        self.table
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key)
            .or_insert(a.clone());
        FixedString(a)
    }
}

/// `RE::BSTArray<T>` and `RE::BSScrapArray<T>` are `Vec<T>`.
pub type Array<T> = Vec<T>;
/// `RE::BSReadWriteLock` with its guards is `std::sync::RwLock`.
pub type ReadWriteLock<T> = RwLock<T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_strings_compare_by_identity_and_ignore_case() {
        let c = StringCache::default();
        let a = c.intern("NPC Head [Head]");
        let b = c.intern("npc head [head]");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "NPC Head [Head]");
        assert_ne!(a, c.intern("NPC Spine [Spn0]"));
    }
}
