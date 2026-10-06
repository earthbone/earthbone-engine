// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic work windows for PRF-14. Fixed scenarios and separate setup
//! follow docs/performance.md and local godotwind/docs/systems/benchmarking.md.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Operations charged at the implementation that performs the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Operation {
    /// Point-location queries, including failed queries.
    NavmeshLocate,
    /// Triangle containment tests, including rejected candidates.
    NavmeshTriangles,
    /// A* queue pops after rejecting stale entries and reachability expansions.
    PathNodes,
    /// Executed PEX opcodes, excluding implicit returns at the end of code.
    PapyrusInstructions,
    /// Invoked native implementations, including scheduler builtins.
    NativeCalls,
    /// CTDA predicates evaluated, excluding short-circuited predicates.
    Conditions,
    /// Physics ray queries, including misses.
    Raycasts,
    /// Host shape sweeps or headless capsule contact queries.
    ShapeCasts,
    /// Record retrieval attempts, including missing forms.
    RecordLookups,
    /// RenderingServer instances created, including lights and MultiMeshes.
    RenderInstances,
}

impl Operation {
    /// Stable order used for snapshots, reports and limit comparisons.
    pub const ALL: [Self; 10] = [
        Self::NavmeshLocate,
        Self::NavmeshTriangles,
        Self::PathNodes,
        Self::PapyrusInstructions,
        Self::NativeCalls,
        Self::Conditions,
        Self::Raycasts,
        Self::ShapeCasts,
        Self::RecordLookups,
        Self::RenderInstances,
    ];

    /// Stable metric name independent of Rust debug formatting.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NavmeshLocate => "navmesh_locate",
            Self::NavmeshTriangles => "navmesh_triangles",
            Self::PathNodes => "path_nodes",
            Self::PapyrusInstructions => "papyrus_instructions",
            Self::NativeCalls => "native_calls",
            Self::Conditions => "conditions",
            Self::Raycasts => "raycasts",
            Self::ShapeCasts => "shape_casts",
            Self::RecordLookups => "record_lookups",
            Self::RenderInstances => "render_instances",
        }
    }
}

/// One frame's deterministic work. No timings, form state or save fields.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkCounts(pub [u64; 10]);

impl WorkCounts {
    /// Number of times this operation ran in the measured window.
    #[must_use]
    pub const fn get(&self, operation: Operation) -> u64 {
        self.0[operation as usize]
    }

    /// Compare every metric, including metrics whose upper bound is zero.
    ///
    /// # Errors
    /// Returns the first exceeded operation, its count and its limit.
    pub fn check(&self, limits: &Self) -> Result<(), (Operation, u64, u64)> {
        for operation in Operation::ALL {
            let count = self.get(operation);
            let limit = limits.get(operation);
            if count > limit {
                return Err((operation, count, limit));
            }
        }
        Ok(())
    }
}

/// Shared capture token. Workers must enter it before doing measured work,
/// and finish before the owner snapshots the frame.
#[derive(Clone)]
pub struct WorkRecorder(Arc<[AtomicU64; 10]>);

thread_local! {
    static ACTIVE: RefCell<Option<WorkRecorder>> = const { RefCell::new(None) };
}

/// Charge deterministic work to the active frame. Outside a frame this is
/// a thread-local check, independent of the timing trace's global switch.
#[inline]
pub fn count(operation: Operation, amount: u64) {
    crate::entity::count(operation, amount);
    ACTIVE.with(|active| {
        if let Some(recorder) = active.borrow().as_ref() {
            recorder.0[operation as usize].fetch_add(amount, Ordering::Relaxed);
        }
    });
}

impl WorkRecorder {
    /// Capture the current frame for a worker task, if a frame is active.
    #[must_use]
    pub fn current() -> Option<Self> {
        ACTIVE.with(|active| active.borrow().clone())
    }

    pub(crate) fn snapshot(&self) -> WorkCounts {
        WorkCounts(std::array::from_fn(|i| self.0[i].load(Ordering::Relaxed)))
    }

    /// Enter this frame on a worker. Dropping the scope restores that thread's
    /// previous capture, including when the worker unwinds.
    pub fn enter(&self) -> WorkFrame {
        let before = self.snapshot();
        let previous = ACTIVE.with(|active| active.replace(Some(self.clone())));
        WorkFrame {
            recorder: self.clone(),
            previous,
            before,
            thread: PhantomData,
        }
    }
}

/// A frame or nested work window on the current thread. Nested windows share
/// the parent's counters and report only work since entry. The scope stays on
/// its entering thread, while its recorder may be sent to workers.
#[must_use]
pub struct WorkFrame {
    recorder: WorkRecorder,
    previous: Option<WorkRecorder>,
    before: WorkCounts,
    thread: PhantomData<Rc<()>>,
}

impl WorkFrame {
    /// Begin a zero-based window, inheriting an active outer frame if present.
    pub fn begin() -> Self {
        let recorder = ACTIVE
            .with(|active| active.borrow().clone())
            .unwrap_or_else(|| WorkRecorder(Arc::new(std::array::from_fn(|_| AtomicU64::new(0)))));
        recorder.enter()
    }

    /// Capture totals since entry without resetting a surrounding frame.
    #[must_use]
    pub fn counts(&self) -> WorkCounts {
        let after = self.recorder.snapshot();
        WorkCounts(std::array::from_fn(|i| after.0[i] - self.before.0[i]))
    }

    /// A worker token belonging to this frame.
    #[must_use]
    pub fn recorder(&self) -> WorkRecorder {
        self.recorder.clone()
    }
}

impl Drop for WorkFrame {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.previous.take()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_14_nested_windows_reset_without_losing_outer_work() {
        let outer = WorkFrame::begin();
        count(Operation::NativeCalls, 2);
        {
            let inner = WorkFrame::begin();
            count(Operation::Conditions, 3);
            assert_eq!(inner.counts().get(Operation::NativeCalls), 0);
            assert_eq!(inner.counts().get(Operation::Conditions), 3);
        }
        assert_eq!(outer.counts().get(Operation::NativeCalls), 2);
        assert_eq!(outer.counts().get(Operation::Conditions), 3);
        drop(outer);
        count(Operation::NativeCalls, 99);
        assert_eq!(WorkFrame::begin().counts(), WorkCounts::default());
    }

    #[test]
    fn prf_14_workers_share_a_frame_and_unrelated_threads_are_isolated() {
        let frame = WorkFrame::begin();
        let recorder = frame.recorder();
        std::thread::scope(|threads| {
            for _ in 0..4 {
                let recorder = recorder.clone();
                threads.spawn(move || {
                    let _scope = recorder.enter();
                    for operation in Operation::ALL {
                        count(operation, 7);
                    }
                });
            }
            threads.spawn(|| {
                let other = WorkFrame::begin();
                count(Operation::NativeCalls, 999);
                assert_eq!(other.counts().get(Operation::NativeCalls), 999);
            });
        });
        assert_eq!(frame.counts(), WorkCounts([28; 10]));
    }

    #[test]
    fn prf_14_every_limit_including_zero_rejects_one_extra_operation() {
        for operation in Operation::ALL {
            let mut counts = WorkCounts::default();
            counts.0[operation as usize] = 1;
            assert_eq!(counts.check(&counts), Ok(()));
            assert_eq!(counts.check(&WorkCounts::default()), Err((operation, 1, 0)));
        }
    }

    #[test]
    fn prf_14_unwinding_restores_the_enclosing_frame() {
        let outer = WorkFrame::begin();
        let result = std::panic::catch_unwind(|| {
            let _inner = WorkFrame::begin();
            count(Operation::Raycasts, 1);
            panic!("fixture unwind");
        });
        assert!(result.is_err());
        count(Operation::Raycasts, 1);
        assert_eq!(outer.counts().get(Operation::Raycasts), 2);
    }
}
