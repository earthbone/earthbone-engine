// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{FrameHistory, FrameSample, Span, Trace, WorkCounts};

#[test]
fn prf_23_tracy_build_keeps_complete_scope_and_frame_adapters() {
    Trace::clear();
    Trace::enable();
    {
        let _span = Span::new("prf_23.breaker", "test");
    }
    Trace::disable();
    assert!(Trace::events()
        .iter()
        .any(|event| event.name == "prf_23.breaker"));

    let mut history = FrameHistory::default();
    history.record(FrameSample::default());
    Trace::plot_work_counts(&WorkCounts::default());
    assert_eq!(history.frames(), 1);
    Trace::clear();
}
