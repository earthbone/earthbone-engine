// SPDX-License-Identifier: GPL-3.0-or-later

//! Work capture does not depend on timing capture and remains thread local.

use soe_trace::{count, Operation, Trace, WorkFrame};

#[test]
fn prf_14_work_capture_ignores_timing_switch_and_unentered_worker_threads() {
    Trace::disable();
    let frame = WorkFrame::begin();
    count(Operation::RecordLookups, 2);
    let recorder = frame.recorder();
    std::thread::scope(|threads| {
        threads.spawn(move || count(Operation::RecordLookups, 100));
        threads.spawn(move || {
            let _scope = recorder.enter();
            count(Operation::RecordLookups, 3);
        });
    });

    assert_eq!(frame.counts().get(Operation::RecordLookups), 5);
    assert_eq!(frame.counts().get(Operation::NativeCalls), 0);
}
