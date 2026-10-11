// SPDX-License-Identifier: GPL-3.0-or-later

//! Background generation and screen-space selection for distant LOD.
//!
//! A request builds the object and terrain products together on a worker and
//! publishes the result only after both are complete. The cache key includes
//! the ordered profile entries and the ESM load-order fingerprint, so profile
//! changes and plugin content changes cannot reuse stale products.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use soe_profile::{LoadOrder, Slot};

use crate::terrain_lod::LodLevel;

/// Stable cache identity for a profile and its resolved plugin contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadOrderHash(u64);

impl LoadOrderHash {
    /// Hash ordered plugin names, enabled state and slot assignments.
    #[must_use]
    pub fn of(order: &LoadOrder) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for entry in &order.entries {
            for byte in entry.name.to_ascii_lowercase().bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash = (hash ^ u64::from(entry.enabled)).wrapping_mul(0x0000_0100_0000_01b3);
            match entry.slot {
                Some(Slot::Regular(slot)) => {
                    hash = (hash ^ 1).wrapping_mul(0x0000_0100_0000_01b3);
                    hash = (hash ^ u64::from(slot)).wrapping_mul(0x0000_0100_0000_01b3);
                }
                Some(Slot::Light(slot)) => {
                    hash = (hash ^ 2).wrapping_mul(0x0000_0100_0000_01b3);
                    hash = (hash ^ u64::from(slot)).wrapping_mul(0x0000_0100_0000_01b3);
                }
                None => hash = (hash ^ 3).wrapping_mul(0x0000_0100_0000_01b3),
            }
            hash = (hash ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self(hash)
    }

    /// Mix the ESM load-order fingerprint into the profile identity.
    #[must_use]
    pub fn with_content_fingerprint(mut self, fingerprint: [u8; 32]) -> Self {
        for byte in fingerprint {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }

    /// Build an identity from the resolved ESM load-order fingerprint.
    #[must_use]
    pub fn from_content_fingerprint(fingerprint: [u8; 32]) -> Self {
        Self::of(&LoadOrder {
            entries: Vec::new(),
            problems: Vec::new(),
        })
        .with_content_fingerprint(fingerprint)
    }

    /// Mix a world and block identity into this load-order key.
    #[must_use]
    pub fn with_block(mut self, world: u32, level: i32, x: i32, y: i32) -> Self {
        for byte in world
            .to_le_bytes()
            .into_iter()
            .chain(level.to_le_bytes())
            .chain(x.to_le_bytes())
            .chain(y.to_le_bytes())
        {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }
}

/// Object and terrain output produced as one load-order snapshot.
#[derive(Debug, Clone)]
pub struct GeneratedLod<TObject, TTerrain> {
    pub object: TObject,
    pub terrain: TTerrain,
}

/// Generated products retained by their load-order identity.
#[derive(Debug)]
pub struct LodBuildCache<T> {
    entries: HashMap<LoadOrderHash, Arc<T>>,
    running: HashMap<LoadOrderHash, Arc<LodBuildWorker<T>>>,
}

impl<T> Default for LodBuildCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            running: HashMap::new(),
        }
    }
}

impl<T> LodBuildCache<T> {
    #[must_use]
    pub fn get(&self, key: LoadOrderHash) -> Option<Arc<T>> {
        self.entries.get(&key).cloned()
    }

    /// Return cached output, share an existing worker or start a new one.
    pub fn request<F>(&mut self, key: LoadOrderHash, build: F) -> LodBuildRequest<T>
    where
        T: Send + Sync + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        if let Some(value) = self.get(key) {
            return LodBuildRequest::Ready(value);
        }
        let worker = self.running.entry(key).or_insert_with(|| {
            Arc::new(LodBuildWorker {
                worker: Mutex::new(Some(thread::spawn(build))),
                result: OnceLock::new(),
            })
        });
        LodBuildRequest::Running {
            key,
            worker: Arc::clone(worker),
        }
    }

    fn publish(&mut self, key: LoadOrderHash, value: Arc<T>) -> Arc<T> {
        self.entries.insert(key, Arc::clone(&value));
        value
    }
}

/// A background worker whose result is shared by requests for the same key.
#[derive(Debug)]
pub struct LodBuildWorker<T> {
    worker: Mutex<Option<JoinHandle<T>>>,
    result: OnceLock<Result<Arc<T>, LodBuildError>>,
}

impl<T> LodBuildWorker<T> {
    fn finish(&self) -> Result<Arc<T>, LodBuildError> {
        self.result
            .get_or_init(|| {
                let worker = self
                    .worker
                    .lock()
                    .map_err(|_| LodBuildError)?
                    .take()
                    .ok_or(LodBuildError)?;
                worker.join().map(Arc::new).map_err(|_| LodBuildError)
            })
            .clone()
    }
}

/// Result of requesting generated LOD.
pub enum LodBuildRequest<T> {
    Ready(Arc<T>),
    Running {
        key: LoadOrderHash,
        worker: Arc<LodBuildWorker<T>>,
    },
}

/// A worker failed before it could publish a complete result.
#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
#[error("LOD generation worker panicked")]
pub struct LodBuildError;

impl<T> LodBuildRequest<T> {
    /// Join the worker and publish its complete result in the cache.
    pub fn finish(self, cache: &mut LodBuildCache<T>) -> Result<Arc<T>, LodBuildError> {
        match self {
            Self::Ready(value) => Ok(value),
            Self::Running { key, worker } => {
                let result = worker.finish();
                if cache
                    .running
                    .get(&key)
                    .is_some_and(|running| Arc::ptr_eq(running, &worker))
                {
                    cache.running.remove(&key);
                }
                result.map(|value| cache.publish(key, value))
            }
        }
    }
}

/// Choose the coarsest level whose projected silhouette error is below
/// the pixel limit. The input order is L4, L8, L16 and L32. Negative errors,
/// non-finite errors and invalid pixel limits are rejected.
#[must_use]
pub fn select_far_level(projected_error_pixels: [f32; 4], pixel_limit: f32) -> Option<LodLevel> {
    let levels = [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32];
    if !pixel_limit.is_finite() || pixel_limit < 0.0 {
        return None;
    }
    levels
        .iter()
        .zip(projected_error_pixels)
        .filter(|(_, error)| error.is_finite() && *error >= 0.0 && *error < pixel_limit)
        .map(|(level, _)| *level)
        .next_back()
}

/// Maximum edge displacement between two projected silhouette bounds in
/// pixels. Each array is `[left, top, right, bottom]`.
#[must_use]
pub fn silhouette_error_pixels(reference: [f32; 4], candidate: [f32; 4]) -> f32 {
    if reference
        .iter()
        .chain(candidate.iter())
        .any(|edge| !edge.is_finite())
    {
        return f32::NAN;
    }
    reference
        .into_iter()
        .zip(candidate)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max)
}
