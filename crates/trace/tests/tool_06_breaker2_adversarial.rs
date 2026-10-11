// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use soe_trace::logging;
use soe_trace::tracing::{self, Dispatch};
use soe_trace::{FrameHistory, FrameSnapshot, Trace};

#[test]
fn tool_06_breaker2_automatic_hitch_export_contains_active_subscriber_logs() {
    let directory = tempfile::tempdir().unwrap();
    let (subscriber, _control) = logging::subscriber("info", None, |_| {}).unwrap();
    let dispatch = Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, || {
        Trace::set_hitch_directory(directory.path());
        Trace::set_hitch_threshold(Duration::from_millis(1)).unwrap();
        let mut history = FrameHistory::default();
        tracing::warn!(target: "papyrus", script = "Probe", "hitch log marker");
        let frame = FrameSnapshot::now();
        std::thread::sleep(Duration::from_millis(5));
        history.record(frame.finish());
        let tail = Instant::now();
        while tail.elapsed() < Duration::from_millis(1100) {
            let frame = FrameSnapshot::now();
            std::thread::sleep(Duration::from_millis(10));
            history.record(frame.finish());
        }
        Trace::wait_for_hitch_exports().unwrap();
    });

    let path = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(json["traceEvents"].as_array().unwrap().iter().any(|event| {
        event["cat"] == "log"
            && event["args"]["fields"]["message"] == "hitch log marker"
            && event["args"]["fields"]["script"] == "Probe"
    }));
}
