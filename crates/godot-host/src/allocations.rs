// SPDX-License-Identifier: GPL-3.0-or-later

//! Counting System allocator using std::alloc's layout and alignment contract.
//! An owner header precedes each payload so frees charge the allocating system.

use soe_trace::{memory_allocated, memory_freed, SystemScope};
use std::alloc::{GlobalAlloc, Layout, System};

pub(crate) struct CountingAllocator;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn wrapped(layout: Layout) -> Option<(Layout, usize)> {
    Layout::new::<soe_trace::System>()
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
        let owner = SystemScope::current_system();
        // SAFETY: The prefix holds an aligned System, and offset is in bounds.
        unsafe {
            base.cast::<soe_trace::System>().write(owner);
        }
        memory_allocated(owner, layout.size());
        // SAFETY: Layout::extend supplies the aligned payload offset.
        unsafe { base.add(offset) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some((outer, offset)) = wrapped(layout) else {
            return;
        };
        // SAFETY: GlobalAlloc requires ptr and layout to match a live allocation.
        let base = unsafe { ptr.sub(offset) };
        // SAFETY: alloc initialized the aligned owner prefix at base.
        let owner = unsafe { base.cast::<soe_trace::System>().read() };
        memory_freed(owner, layout.size());
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
        let owner = unsafe { old_base.cast::<soe_trace::System>().read() };
        // SAFETY: Both layouts retain the same alignment. Realloc preserves the
        // prefix and min(old, new) payload, whose offset also stays the same.
        let new_base = unsafe { System.realloc(old_base, old_outer, new_outer.size()) };
        if new_base.is_null() {
            return new_base;
        }
        memory_freed(owner, layout.size());
        memory_allocated(owner, size);
        // SAFETY: The successful allocation contains the extended new layout.
        unsafe { new_base.add(new_offset) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_trace::{FrameSnapshot, System as Slot};

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
        let owner = unsafe { data.as_ptr().sub(offset).cast::<Slot>().read() };
        assert_eq!(owner, Slot::Save);
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
}
