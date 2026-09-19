// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded scoped work with input-order joins, following the data source's
//! load-order contract. A failed chunk cannot become a shorter result list.

use super::DataSourceError;

pub(super) fn ordered_map<T: Sync, U: Send>(
    items: &[T],
    threads: usize,
    apply: impl Fn(&T) -> U + Sync,
) -> Result<Vec<U>, DataSourceError> {
    let threads = threads.max(1).min(items.len().max(1));
    if threads == 1 {
        return Ok(items.iter().map(apply).collect());
    }
    std::thread::scope(|scope| {
        let apply = &apply;
        let handles: Vec<_> = items
            .chunks(items.len().div_ceil(threads))
            .map(|chunk| {
                scope.spawn(move || {
                    let _worker =
                        soe_trace::Span::with_payload("host.worker", "host", chunk.len() as u64);
                    chunk.iter().map(apply).collect::<Vec<_>>()
                })
            })
            .collect();
        let mut output = Vec::with_capacity(items.len());
        let mut failed = false;
        for handle in handles {
            match handle.join() {
                Ok(part) => output.extend(part),
                Err(_) => failed = true,
            }
        }
        if failed {
            Err(DataSourceError::WorkerPanicked)
        } else {
            Ok(output)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::ordered_map;
    use crate::data_source::DataSourceError;

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
