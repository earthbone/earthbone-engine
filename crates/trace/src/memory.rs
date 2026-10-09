// SPDX-License-Identifier: GPL-3.0-or-later

//! Allocation accounting for the host counting allocator. Ownership survives
//! scope changes and thread transfer, unlike a scoped allocation delta.

use crate::{System, SYSTEM_COUNT};
use std::sync::atomic::{AtomicU64, Ordering};

static LIVE: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];
static CALLS: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];
static BYTES: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];

/// Cumulative successful allocations and currently owned requested bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemorySnapshot {
    pub live_bytes: [u64; SYSTEM_COUNT],
    pub allocation_calls: [u64; SYSTEM_COUNT],
    pub allocated_bytes: [u64; SYSTEM_COUNT],
}

impl MemorySnapshot {
    /// Process-wide Rust allocation counters, excluding Godot's own heap and GPU.
    pub fn now() -> Self {
        Self {
            live_bytes: std::array::from_fn(|index| LIVE[index].load(Ordering::Relaxed)),
            allocation_calls: std::array::from_fn(|index| CALLS[index].load(Ordering::Relaxed)),
            allocated_bytes: std::array::from_fn(|index| BYTES[index].load(Ordering::Relaxed)),
        }
    }
}

/// Charge a successful allocation to the owner retained by its allocator header.
pub fn memory_allocated(system: System, bytes: usize) {
    LIVE[system as usize].fetch_add(bytes as u64, Ordering::Relaxed);
    CALLS[system as usize].fetch_add(1, Ordering::Relaxed);
    BYTES[system as usize].fetch_add(bytes as u64, Ordering::Relaxed);
}

/// Release requested bytes from their original owner, even on another thread.
pub fn memory_freed(system: System, bytes: usize) {
    LIVE[system as usize].fetch_sub(bytes as u64, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameSnapshot;

    #[test]
    fn prf_20_memory_frames_report_live_bytes_and_calls_for_every_system() {
        let before = MemorySnapshot::now();
        let frame = FrameSnapshot::now();
        for system in System::ALL {
            memory_allocated(system, 17);
        }
        let sample = frame.finish();
        for system in System::ALL {
            let index = system as usize;
            assert_eq!(sample.live_bytes[index], before.live_bytes[index] + 17);
            assert_eq!(sample.allocation_calls[index], 1);
            memory_freed(system, 17);
        }
        assert_eq!(MemorySnapshot::now().live_bytes, before.live_bytes);
        let idle = FrameSnapshot::now().finish();
        assert_eq!(idle.allocation_calls, [0; SYSTEM_COUNT]);
    }
}
