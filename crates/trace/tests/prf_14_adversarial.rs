// SPDX-License-Identifier: GPL-3.0-or-later

//! Counter contract checks across every deterministic work operation.

use soe_trace::{count, Operation, WorkCounts, WorkFrame};

#[test]
fn prf_14_all_operations_count_in_nested_windows_and_respect_inclusive_limits() {
    let outer = WorkFrame::begin();
    for operation in Operation::ALL {
        count(operation, 2);
    }
    {
        let inner = WorkFrame::begin();
        count(Operation::NavmeshLocate, 1);
        count(Operation::ShapeCasts, 3);
        assert_eq!(inner.counts().get(Operation::NavmeshLocate), 1);
        assert_eq!(inner.counts().get(Operation::ShapeCasts), 3);
        assert_eq!(inner.counts().get(Operation::NativeCalls), 0);
    }

    let totals = outer.counts();
    assert_eq!(totals.get(Operation::NavmeshLocate), 3);
    assert_eq!(totals.get(Operation::ShapeCasts), 5);
    assert!(Operation::ALL
        .iter()
        .all(|operation| totals.get(*operation) >= 2));
    let mut limits = totals.clone();
    assert_eq!(totals.check(&limits), Ok(()));
    limits.0[Operation::NativeCalls as usize] -= 1;
    assert_eq!(totals.check(&limits), Err((Operation::NativeCalls, 2, 1)));
}

#[test]
fn prf_14_work_outside_a_dropped_frame_does_not_leak_into_the_next_frame() {
    {
        let frame = WorkFrame::begin();
        count(Operation::RecordLookups, 7);
        assert_eq!(frame.counts().get(Operation::RecordLookups), 7);
    }
    count(Operation::RecordLookups, 100);
    let next = WorkFrame::begin();
    assert_eq!(next.counts(), WorkCounts::default());
}
