// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{with_timing_suspended, FrameSnapshot, System, SystemScope};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

#[test]
fn prf_12_suspension_excludes_an_already_active_system_scope() {
    let frame = FrameSnapshot::now();
    let active = SystemScope::new(System::Papyrus);
    std::thread::sleep(Duration::from_millis(8));
    with_timing_suspended(|| std::thread::sleep(Duration::from_millis(30)));
    std::thread::sleep(Duration::from_millis(8));
    drop(active);

    let sample = frame.finish();
    assert!(
        sample.milliseconds(System::Papyrus) < 25.0,
        "suspended work was charged: {:.3} ms",
        sample.milliseconds(System::Papyrus)
    );
}

#[test]
fn prf_12_nested_suspension_restores_only_at_the_outer_boundary() {
    assert!(FrameSnapshot::when_recording().is_some());
    with_timing_suspended(|| {
        assert!(FrameSnapshot::when_recording().is_none());
        with_timing_suspended(|| assert!(FrameSnapshot::when_recording().is_none()));
        assert!(FrameSnapshot::when_recording().is_none());
    });
    assert!(FrameSnapshot::when_recording().is_some());
}

#[test]
#[allow(clippy::panic, reason = "Exercises recording restoration after unwind")]
fn prf_12_suspension_restores_recording_after_unwind() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_timing_suspended(|| panic!("calibration abort"));
    }));
    assert!(result.is_err());
    assert!(FrameSnapshot::when_recording().is_some());
}
