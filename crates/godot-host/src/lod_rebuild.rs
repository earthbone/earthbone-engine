// SPDX-License-Identifier: GPL-3.0-or-later

//! Load-order keyed background work and screen-space selection for generated LOD.
//!
//! The cache key includes the ordered plugin identity, enabled state and slot.
//! A caller can submit object and terrain products together so one profile
//! change cannot publish a mixed pair. The worker is independent of Godot and
//! can build decoded mesh data away from the scene thread.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use soe_profile::{LoadOrder, Slot};

use crate::terrain_lod::LodLevel;

/// Stable 64-bit identity for the active plugin order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadOrderHash(u64);

impl LoadOrderHash {
    /// Hashes the ordered plugin list, including disabled entries and slot
    /// assignments because either can change resolved FormIDs or winners.
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
}

/// Cache of generated LOD products, indexed by the load order that produced
/// them. Values are shared with streamers without copying mesh arrays.
#[derive(Debug)]
pub struct LodBuildCache<T> {
    entries: HashMap<LoadOrderHash, Arc<T>>,
}

impl<T> Default for LodBuildCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> LodBuildCache<T> {
    #[must_use]
    pub fn get(&self, key: LoadOrderHash) -> Option<Arc<T>> {
        self.entries.get(&key).cloned()
    }

    /// Start generation on a worker. A cache hit returns immediately without
    /// starting the closure; a miss creates a job that publishes under `key`
    /// only after the entire result has been built.
    pub fn request<F>(&mut self, key: LoadOrderHash, build: F) -> LodBuildRequest<T>
    where
        T: Send + Sync + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        if let Some(value) = self.get(key) {
            return LodBuildRequest::Ready(value);
        }
        LodBuildRequest::Running {
            key,
            worker: thread::spawn(build),
        }
    }

    pub fn publish(&mut self, key: LoadOrderHash, value: T) -> Arc<T> {
        let value = Arc::new(value);
        self.entries.insert(key, Arc::clone(&value));
        value
    }
}

/// The result of requesting background generation.
pub enum LodBuildRequest<T> {
    Ready(Arc<T>),
    Running {
        key: LoadOrderHash,
        worker: JoinHandle<T>,
    },
}

/// A background LOD builder panicked before publishing a result.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("LOD generation worker panicked")]
pub struct LodBuildError;

impl<T> LodBuildRequest<T> {
    /// Completes a worker and atomically publishes its complete result. The
    /// caller can keep rendering the previous cache entry until this returns.
    pub fn finish(self, cache: &mut LodBuildCache<T>) -> Result<Arc<T>, LodBuildError> {
        match self {
            Self::Ready(value) => Ok(value),
            Self::Running { key, worker } => match worker.join() {
                Ok(value) => Ok(cache.publish(key, value)),
                Err(_) => Err(LodBuildError),
            },
        }
    }
}

/// Selects the coarsest level whose measured projected silhouette error is
/// within `pixel_limit`. Errors are ordered as L4, L8, L16 and L32.
#[must_use]
pub fn select_far_level(projected_error_pixels: [f32; 4], pixel_limit: f32) -> LodLevel {
    let levels = [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32];
    levels
        .iter()
        .zip(projected_error_pixels)
        .filter(|(_, error)| error.is_finite() && *error <= pixel_limit)
        .map(|(level, _)| *level)
        .next_back()
        .unwrap_or(LodLevel::L4)
}

/// Maximum projected edge displacement between two axis-aligned silhouette
/// bounds in pixel coordinates. The bounds are `[left, top, right, bottom]`.
#[must_use]
pub fn silhouette_error_pixels(reference: [f32; 4], candidate: [f32; 4]) -> f32 {
    reference
        .into_iter()
        .zip(candidate)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max)
}
