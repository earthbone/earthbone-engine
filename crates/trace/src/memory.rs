// SPDX-License-Identifier: GPL-3.0-or-later

//! Allocation accounting for the host counting allocator. Ownership survives
//! scope changes and thread transfer, unlike a scoped allocation delta.

use crate::{System, SYSTEM_COUNT};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

static LIVE: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];
static CALLS: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];
static BYTES: [AtomicU64; SYSTEM_COUNT] = [const { AtomicU64::new(0) }; SYSTEM_COUNT];
static TOTAL: AtomicU64 = AtomicU64::new(0);
static PEAK_EVER: AtomicU64 = AtomicU64::new(0);
static PEAK_INTERVAL: AtomicU64 = AtomicU64::new(0);

/// Total live Rust bytes and the highest totals seen. The interval peak is
/// cleared by each `rust_heap_peaks` call, so a report covers the time since
/// the previous one. A short spike that the live counters miss shows up here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RustHeapPeaks {
    pub live_bytes: u64,
    pub peak_since_start_bytes: u64,
    pub peak_since_last_report_bytes: u64,
}

/// Read the Rust heap totals and restart the interval peak at the current total.
pub fn rust_heap_peaks() -> RustHeapPeaks {
    let live_bytes = TOTAL.load(Ordering::Relaxed);
    let peak_since_last_report_bytes = PEAK_INTERVAL.swap(live_bytes, Ordering::Relaxed);
    RustHeapPeaks {
        live_bytes,
        peak_since_start_bytes: PEAK_EVER.load(Ordering::Relaxed),
        peak_since_last_report_bytes: peak_since_last_report_bytes.max(live_bytes),
    }
}

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
    let total = TOTAL.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
    if total > PEAK_INTERVAL.load(Ordering::Relaxed) {
        PEAK_INTERVAL.fetch_max(total, Ordering::Relaxed);
    }
    if total > PEAK_EVER.load(Ordering::Relaxed) {
        PEAK_EVER.fetch_max(total, Ordering::Relaxed);
    }
}

/// Release requested bytes from their original owner, even on another thread.
pub fn memory_freed(system: System, bytes: usize) {
    LIVE[system as usize].fetch_sub(bytes as u64, Ordering::Relaxed);
    TOTAL.fetch_sub(bytes as u64, Ordering::Relaxed);
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

    #[test]
    fn prf_20_heap_peak_keeps_a_spike_that_was_freed_before_the_report() {
        let _ = rust_heap_peaks();
        let base = rust_heap_peaks().live_bytes;
        memory_allocated(System::Other, 1 << 30);
        memory_freed(System::Other, 1 << 30);
        let report = rust_heap_peaks();
        assert!(report.peak_since_last_report_bytes >= base + (1 << 30));
        assert!(report.peak_since_start_bytes >= base + (1 << 30));
        let next = rust_heap_peaks();
        assert!(next.peak_since_last_report_bytes < base + (1 << 29));
    }
}

/// Allocation provenance for the large retained host payloads.
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum PayloadOwner {
    Other,
    Source,
    ActorGraph,
    ActorClips,
    Scripts,
    Movement,
    Gameplay,
    Cell,
}

impl PayloadOwner {
    pub const ALL: [Self; 8] = [
        Self::Other,
        Self::Source,
        Self::ActorGraph,
        Self::ActorClips,
        Self::Scripts,
        Self::Movement,
        Self::Gameplay,
        Self::Cell,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::Source => "source",
            Self::ActorGraph => "actor_graph",
            Self::ActorClips => "actor_clips",
            Self::Scripts => "scripts",
            Self::Movement => "movement",
            Self::Gameplay => "gameplay",
            Self::Cell => "cell",
        }
    }
}

thread_local! {
    static OWNER: Cell<PayloadOwner> = const { Cell::new(PayloadOwner::Other) };
}

/// Scoped payload provenance, restored on drop and forwarded explicitly to workers.
pub struct PayloadOwnerScope(PayloadOwner);

impl PayloadOwnerScope {
    pub fn new(owner: PayloadOwner) -> Self {
        Self(OWNER.replace(owner))
    }

    pub fn current() -> PayloadOwner {
        OWNER.get()
    }
}

impl Drop for PayloadOwnerScope {
    fn drop(&mut self) {
        OWNER.set(self.0);
    }
}
