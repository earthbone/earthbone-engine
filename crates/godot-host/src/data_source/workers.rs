// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent bounded workers with input-order results, following the data
//! source's load-order contract. A failed batch cannot shorten its result list.

use super::DataSourceError;
use rayon::prelude::*;
use rayon::ThreadPool;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

type CachedPool = Arc<Mutex<Option<(usize, Arc<ThreadPool>)>>>;

/// Reuses bounded worker threads across data-source batches.
#[derive(Clone, Default)]
pub(super) struct WorkerPools {
    pool: CachedPool,
}

impl WorkerPools {
    pub(super) fn ordered_map<T: Sync, U: Send>(
        &self,
        items: &[T],
        threads: usize,
        apply: impl Fn(&T) -> U + Sync,
    ) -> Result<Vec<U>, DataSourceError> {
        let threads = threads.max(1).min(items.len().max(1));
        if threads == 1 {
            return Ok(items.iter().map(apply).collect());
        }
        let pool = self.pool(threads)?;
        let chunk_size = items.len().div_ceil(threads);
        catch_unwind(AssertUnwindSafe(|| {
            pool.install(|| {
                items
                    .par_chunks(chunk_size)
                    .map(|chunk| {
                        let _worker = soe_trace::Span::with_payload(
                            "host.worker",
                            "host",
                            chunk.len() as u64,
                        );
                        chunk.iter().map(&apply).collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .flatten()
                    .collect()
            })
        }))
        .map_err(|_| DataSourceError::WorkerPanicked)
    }

    fn pool(&self, threads: usize) -> Result<Arc<ThreadPool>, DataSourceError> {
        let mut current = self
            .pool
            .lock()
            .map_err(|_| DataSourceError::WorkerPanicked)?;
        if let Some((current_threads, pool)) = current.as_ref() {
            if *current_threads == threads {
                return Ok(Arc::clone(pool));
            }
        }

        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(DataSourceError::WorkerPoolBuild)?,
        );
        *current = Some((threads, Arc::clone(&pool)));
        Ok(pool)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::WorkerPools;
    use crate::data_source::DataSourceError;

    fn ordered_map<T: Sync, U: Send>(
        items: &[T],
        threads: usize,
        apply: impl Fn(&T) -> U + Sync,
    ) -> Result<Vec<U>, DataSourceError> {
        WorkerPools::default().ordered_map(items, threads, apply)
    }

    #[test]
    fn worker_counts_preserve_input_order_including_empty_work() {
        for len in [0, 1, 2, 17] {
            let input: Vec<_> = (0..len).collect();
            let expected: Vec<_> = input.iter().map(|i| i * i).collect();
            for workers in [0, 1, 2, 8, 32] {
                let actual = ordered_map(&input, workers, |i| {
                    std::thread::yield_now();
                    i * i
                })
                .unwrap();
                assert_eq!(actual, expected, "len {len}, workers {workers}");
            }
        }
    }

    #[test]
    fn two_workers_never_execute_three_items_at_once() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        ordered_map(&[0; 40], 2, |_| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            std::thread::yield_now();
            active.fetch_sub(1, Ordering::SeqCst);
        })
        .unwrap();
        assert!(peak.load(Ordering::SeqCst) <= 2, "worker limit exceeded");
    }

    #[test]
    fn panicked_chunks_fail_the_batch_after_joining_every_worker() {
        let completed = AtomicUsize::new(0);
        let result = ordered_map(&[0, 1, 2, 3, 4, 5], 3, |i| {
            assert!(*i >= 4, "injected failure in each of two chunks");
            completed.fetch_add(1, Ordering::SeqCst);
            *i
        });
        assert!(matches!(result, Err(DataSourceError::WorkerPanicked)));
        assert_eq!(completed.load(Ordering::SeqCst), 2, "last chunk was joined");
    }

    #[test]
    fn prf_05_worker_reuse_cost_is_measured_against_scoped_startup() {
        use std::time::{Duration, Instant};

        fn median(mut samples: Vec<Duration>) -> Duration {
            samples.sort_unstable();
            samples[samples.len() / 2]
        }

        let input: Vec<_> = (0..512usize).collect();
        let pool = WorkerPools::default();
        let construction_start = Instant::now();
        let first = pool.ordered_map(&input, 4, |value| value * 2).unwrap();
        let construction_plus_first_batch = construction_start.elapsed();
        assert_eq!(first.len(), input.len());

        let mut scoped = Vec::with_capacity(21);
        let mut reused = Vec::with_capacity(21);
        for _ in 0..21 {
            let start = Instant::now();
            let scoped_result = std::thread::scope(|scope| {
                let handles: Vec<_> = input
                    .chunks(input.len().div_ceil(4))
                    .map(|chunk| {
                        scope.spawn(move || chunk.iter().map(|value| value * 2).sum::<usize>())
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap())
                    .sum::<usize>()
            });
            scoped.push(start.elapsed());

            let start = Instant::now();
            let reused_result = pool
                .ordered_map(&input, 4, |value| value * 2)
                .unwrap()
                .into_iter()
                .sum::<usize>();
            reused.push(start.elapsed());
            assert_eq!(scoped_result, reused_result);
        }

        let scoped_median = median(scoped);
        let reused_median = median(reused);
        eprintln!(
            "prf_05 worker startup: pool create plus first batch {construction_plus_first_batch:?}, scoped batch median {scoped_median:?}, reused pool batch median {reused_median:?}"
        );
        assert!(
            pool.pool(4).is_ok(),
            "the configured worker pool remains available"
        );
    }

    #[test]
    fn prf_05_worker_pool_cache_replaces_the_previous_worker_count() {
        let pools = WorkerPools::default();
        let previous = pools.pool(2).unwrap();
        let current = pools.pool(4).unwrap();
        let cached = pools.pool.lock().unwrap();
        let (threads, cached_pool) = cached.as_ref().unwrap();

        assert_eq!(*threads, 4, "only the active worker count stays cached");
        assert!(
            !Arc::ptr_eq(&previous, cached_pool),
            "a new count replaces the previous pool"
        );
        assert!(Arc::ptr_eq(&current, cached_pool));
    }

    #[test]
    fn parallel_archive_construction_preserves_last_archive_wins() {
        use soe_bsa::{Archive, Vfs};
        use std::path::Path;

        // Reduced from bsa::archive::testdata::sse_bsa, which is test-private
        // to that crate. One folder, one uncompressed file, two payload bytes.
        fn archive_bytes(payload: [u8; 2]) -> Vec<u8> {
            let mut bytes = b"BSA\0".to_vec();
            for value in [105u32, 36, 3, 1, 1, 7, 9, 0] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&0u64.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&0u64.to_le_bytes());
            bytes.push(7);
            bytes.extend_from_slice(b"meshes\0");
            bytes.extend_from_slice(&0u64.to_le_bytes());
            bytes.extend_from_slice(&2u32.to_le_bytes());
            bytes.extend_from_slice(&93u32.to_le_bytes());
            bytes.extend_from_slice(b"same.nif\0");
            assert_eq!(bytes.len(), 93);
            bytes.extend_from_slice(&payload);
            bytes
        }
        for workers in [1, 2, 8] {
            let archives = ordered_map(&[0u8, 1, 2, 3], workers, |i| {
                std::thread::yield_now();
                Archive::from_bytes(Path::new("test.bsa"), archive_bytes([*i, 42])).unwrap()
            })
            .unwrap();
            let mut vfs = Vfs::new();
            for archive in archives {
                vfs.add_archive(archive);
            }
            assert_eq!(vfs.read("Meshes\\Same.NIF").unwrap(), [3, 42]);
        }
    }
}
