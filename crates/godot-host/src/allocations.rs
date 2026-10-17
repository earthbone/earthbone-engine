// SPDX-License-Identifier: GPL-3.0-or-later

//! Counting System allocator using std::alloc's layout and alignment contract.
//! An owner header precedes each payload so frees charge its original system
//! and payload provenance. The header retains the original word size.

use soe_trace::{memory_allocated, memory_freed, SystemScope};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) use soe_trace::{PayloadOwner as Owner, PayloadOwnerScope as OwnerScope};

static OWNED: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
#[cfg(all(target_os = "linux", target_env = "gnu"))]
static MALLOC_OWNED: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

#[cfg(all(target_os = "linux", target_env = "gnu"))]
extern "C" {
    fn malloc_usable_size(pointer: *mut std::ffi::c_void) -> usize;
}

#[cfg(any(feature = "godot", test))]
pub(crate) fn live_bytes(owner: Owner) -> u64 {
    OWNED[owner as usize].load(Ordering::Relaxed)
}

/// glibc usable bytes include the provenance header and allocation rounding.
#[cfg(all(target_os = "linux", target_env = "gnu", any(feature = "godot", test)))]
pub(crate) fn malloc_live_bytes(owner: Owner) -> u64 {
    MALLOC_OWNED[owner as usize].load(Ordering::Relaxed)
}

/// Read only a live base pointer supplied by System, before its release.
unsafe fn malloc_bytes(base: *mut u8) -> u64 {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: System uses glibc malloc or posix_memalign on this target.
        unsafe { malloc_usable_size(base.cast()) as u64 }
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        let _ = base;
        0
    }
}

fn malloc_allocated(owner: Owner, bytes: u64) {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    MALLOC_OWNED[owner as usize].fetch_add(bytes, Ordering::Relaxed);
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    let _ = (owner, bytes);
}

fn malloc_freed(owner: Owner, bytes: u64) {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    MALLOC_OWNED[owner as usize].fetch_sub(bytes, Ordering::Relaxed);
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    let _ = (owner, bytes);
}

/// Pack provenance into the existing system word without increasing its size.
#[derive(Clone, Copy)]
struct Header(usize);

impl Header {
    fn new() -> Self {
        Self(SystemScope::current_system() as usize | (OwnerScope::current() as usize) << 4)
    }

    fn system(self) -> soe_trace::System {
        soe_trace::System::ALL[self.0 & 15]
    }

    fn owner(self) -> Owner {
        Owner::ALL[self.0 >> 4]
    }

    fn allocated(self, bytes: usize) {
        memory_allocated(self.system(), bytes);
        OWNED[self.owner() as usize].fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn freed(self, bytes: usize) {
        memory_freed(self.system(), bytes);
        OWNED[self.owner() as usize].fetch_sub(bytes as u64, Ordering::Relaxed);
    }
}

pub(crate) struct CountingAllocator;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn wrapped(layout: Layout) -> Option<(Layout, usize)> {
    Layout::new::<Header>()
        .extend(layout)
        .ok()
        .map(|(layout, offset)| (layout.pad_to_align(), offset))
}

// SAFETY: Every payload has its supplied alignment and size inside the expanded
// System allocation. The prefix is initialized before publishing the pointer.
#[allow(clippy::cast_ptr_alignment)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some((outer, offset)) = wrapped(layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: The validated expanded layout is passed unchanged to System.
        let base = unsafe { System.alloc(outer) };
        if base.is_null() {
            return base;
        }
        let owner = Header::new();
        // SAFETY: The prefix holds an aligned Header, and offset is in bounds.
        unsafe {
            base.cast::<Header>().write(owner);
        }
        owner.allocated(layout.size());
        // SAFETY: base is a successful live System allocation.
        malloc_allocated(owner.owner(), unsafe { malloc_bytes(base) });
        // SAFETY: Layout::extend supplies the aligned payload offset.
        unsafe { base.add(offset) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some((outer, offset)) = wrapped(layout) else {
            return;
        };
        // SAFETY: GlobalAlloc requires ptr and layout to match a live allocation.
        let base = unsafe { ptr.sub(offset) };
        // SAFETY: alloc initialized the aligned header at base.
        let owner = unsafe { base.cast::<Header>().read() };
        owner.freed(layout.size());
        // SAFETY: base is still live until System.dealloc below.
        malloc_freed(owner.owner(), unsafe { malloc_bytes(base) });
        // SAFETY: This is the same base pointer and layout passed to System.alloc.
        unsafe {
            System.dealloc(base, outer);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let Ok(new_layout) = Layout::from_size_align(size, layout.align()) else {
            return std::ptr::null_mut();
        };
        let Some((old_outer, old_offset)) = wrapped(layout) else {
            return std::ptr::null_mut();
        };
        let Some((new_outer, new_offset)) = wrapped(new_layout) else {
            return std::ptr::null_mut();
        };
        // SAFETY: ptr came from this allocator with the supplied layout.
        let old_base = unsafe { ptr.sub(old_offset) };
        // SAFETY: The prefix is initialized for every successful allocation.
        let owner = unsafe { old_base.cast::<Header>().read() };
        // SAFETY: realloc has not yet changed or freed this live System pointer.
        let old_malloc_bytes = unsafe { malloc_bytes(old_base) };
        // SAFETY: Both layouts retain the same alignment. Realloc preserves the
        // prefix and min(old, new) payload, whose offset also stays the same.
        let new_base = unsafe { System.realloc(old_base, old_outer, new_outer.size()) };
        if new_base.is_null() {
            return new_base;
        }
        owner.freed(layout.size());
        owner.allocated(size);
        malloc_freed(owner.owner(), old_malloc_bytes);
        // SAFETY: the successful realloc supplies a live System base pointer.
        malloc_allocated(owner.owner(), unsafe { malloc_bytes(new_base) });
        // SAFETY: The successful allocation contains the extended new layout.
        unsafe { new_base.add(new_offset) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_trace::{FrameSnapshot, System as Slot};

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn glibc_footprint_follows_reallocation_and_cross_thread_release() {
        const MARKER: &str = "SOE_MALLOC_FOOTPRINT_CHILD";
        if std::env::var_os(MARKER).is_some() {
            let base = malloc_live_bytes(Owner::ActorClips);
            let requested = live_bytes(Owner::ActorClips);
            let mut bytes = {
                let _owner = OwnerScope::new(Owner::ActorClips);
                vec![1_u8; 37]
            };
            assert_eq!(live_bytes(Owner::ActorClips), requested + 37);
            assert!(
                malloc_live_bytes(Owner::ActorClips)
                    >= base + 37 + std::mem::size_of::<Header>() as u64
            );
            bytes.reserve_exact(32 * 1024);
            assert_eq!(
                live_bytes(Owner::ActorClips),
                requested + bytes.capacity() as u64
            );
            assert!(malloc_live_bytes(Owner::ActorClips) >= base + bytes.capacity() as u64);
            std::thread::spawn(move || {
                assert_eq!(bytes[0], 1);
                drop(bytes);
            })
            .join()
            .unwrap();
            assert_eq!(malloc_live_bytes(Owner::ActorClips), base);
            assert_eq!(live_bytes(Owner::ActorClips), requested);
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "allocations::tests::glibc_footprint_follows_reallocation_and_cross_thread_release",
                "--test-threads=1",
            ])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[allow(clippy::cast_ptr_alignment)]
    fn prf_20_allocator_retains_owner_across_realloc_scope_and_thread_transfer() {
        let frame = FrameSnapshot::now();
        let mut data;
        {
            let _scope = SystemScope::new(Slot::Save);
            data = Vec::<u8>::with_capacity(37);
            data.resize(37, 5);
        }
        {
            let _scope = SystemScope::new(Slot::Audio);
            data.reserve_exact(1000);
        }
        let layout = Layout::array::<u8>(data.capacity()).unwrap();
        let (_, offset) = wrapped(layout).unwrap();
        // SAFETY: Vec's payload was allocated by GLOBAL with this capacity.
        let owner = unsafe { data.as_ptr().sub(offset).cast::<Header>().read() };
        assert_eq!(owner.system(), Slot::Save);
        std::thread::spawn(move || {
            assert_eq!(data[0], 5);
            drop(data);
        })
        .join()
        .unwrap();
        assert!(frame.finish().allocation_calls[Slot::Save as usize] >= 2);
    }

    #[test]
    fn prf_20_allocator_preserves_large_alignment_and_zeroed_payload() {
        let layout = Layout::from_size_align(513, 4096).unwrap();
        // SAFETY: This test pairs each pointer with its exact allocation layout.
        unsafe {
            let ptr = GLOBAL.alloc_zeroed(layout);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 4096, 0);
            assert_eq!(std::slice::from_raw_parts(ptr, 513), &[0; 513]);
            let grown = GLOBAL.realloc(ptr, layout, 1025);
            assert!(!grown.is_null());
            assert_eq!(grown as usize % 4096, 0);
            assert_eq!(std::slice::from_raw_parts(grown, 513), &[0; 513]);
            GLOBAL.dealloc(grown, Layout::from_size_align(1025, 4096).unwrap());
        }
    }

    #[test]
    fn payload_owner_survives_realloc_and_cross_thread_release() {
        assert_eq!(std::mem::size_of::<Header>(), std::mem::size_of::<Slot>());
        assert!(Slot::ALL.len() <= 16);
        let mut data = {
            let _owner = OwnerScope::new(Owner::ActorClips);
            vec![5_u8; 37]
        };
        {
            let _owner = OwnerScope::new(Owner::Scripts);
            data.reserve_exact(1000);
        }
        let layout = Layout::array::<u8>(data.capacity()).unwrap();
        let (_, offset) = wrapped(layout).unwrap();
        // SAFETY: The Vec uses this allocator with the measured capacity.
        let bytes = unsafe {
            data.as_ptr()
                .sub(offset)
                .cast::<[u8; std::mem::size_of::<usize>()]>()
                .read_unaligned()
        };
        let header = Header(usize::from_ne_bytes(bytes));
        assert_eq!(header.owner() as usize, Owner::ActorClips as usize);
        std::thread::spawn(move || {
            assert_eq!(data[0], 5);
            drop(data);
        })
        .join()
        .unwrap();
    }
}
