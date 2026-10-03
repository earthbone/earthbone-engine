// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use soe_trace::{with_timing_suspended, FrameSnapshot, System, SystemScope};
use std::time::Duration;

#[test]
fn prf_12_suspended_time_inside_an_active_system_scope_is_excluded() {
    let frame = FrameSnapshot::now();
    let active = SystemScope::new(System::Papyrus);
    std::thread::sleep(Duration::from_millis(8));
    with_timing_suspended(|| std::thread::sleep(Duration::from_millis(40)));
    std::thread::sleep(Duration::from_millis(8));
    drop(active);

    let sample = frame.finish();
    assert!(sample.frame_ns >= 50_000_000);
    assert!(
        sample.milliseconds(System::Papyrus) < 30.0,
        "suspended interval was charged: {:.3} ms",
        sample.milliseconds(System::Papyrus)
    );
}
