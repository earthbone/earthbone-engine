// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_trace::{FrameHistory, FrameSample, Trace};
use std::time::Duration;

#[test]
fn prf_18_breaker_a_hitch_in_headless_history_exports_only_after_tail_and_respects_threshold() {
    let directory = std::env::temp_dir().join(format!("prf-18-breaker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    Trace::set_hitch_directory(&directory);
    Trace::set_hitch_threshold(Duration::from_millis(20)).unwrap();

    let mut history = FrameHistory::default();
    history.record(FrameSample { frame_ns: 20_000_000, ..FrameSample::default() });
    std::thread::sleep(Duration::from_millis(20));
    assert!(!directory.exists(), "threshold is strict and a matching frame is not a hitch");
    history.record(FrameSample { frame_ns: 20_000_001, ..FrameSample::default() });
    std::thread::sleep(Duration::from_millis(1_050));
    history.record(FrameSample::default());
    Trace::wait_for_hitch_exports().unwrap();
    assert!(directory.exists(), "the headless frame recorder must export the hitch");
    std::fs::remove_dir_all(directory).unwrap();
}
