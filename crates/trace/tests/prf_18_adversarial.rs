// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Adversarial pressure check for the bounded PRF-18 recorder.

use soe_trace::{FrameHistory, FrameSample, Trace};

#[test]
fn prf_18_ring_pressure_reports_evictions_and_keeps_recent_spans() {
    let directory = std::env::temp_dir().join(format!(
        "prf-18-adversarial-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_dir_all(&directory);
    Trace::set_hitch_directory(&directory);

    let mut history = FrameHistory::default();
    for index in 0..70_000 {
        let _span = soe_trace::Span::new(
            if index < 10 { "early.pressure" } else { "recent.pressure" },
            "adversarial",
        );
    }
    history.record(FrameSample {
        frame_ns: 50_000_000,
        ..FrameSample::default()
    });
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(1_050) {
        history.record(FrameSample::default());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Trace::wait_for_hitch_exports().expect("hitch writer should finish");

    let json_path = std::fs::read_dir(&directory)
        .expect("hitch directory should exist")
        .map(|entry| entry.expect("directory entry should be readable").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("hitch JSON should be written");
    let json = std::fs::read_to_string(json_path).expect("hitch JSON should be readable");
    let overwritten: u64 = json
        .split("\"overwritten_events\":")
        .nth(1)
        .expect("dump should report recorder pressure")
        .split('}')
        .next()
        .expect("overwrite count should be present")
        .parse()
        .expect("overwrite count should be numeric");

    assert!(overwritten > 0, "ring evictions should be reported");
    assert!(json.contains("recent.pressure"));
    assert!(!json.contains("early.pressure"));
    std::fs::remove_dir_all(directory).expect("temporary hitch directory should be removed");
}
