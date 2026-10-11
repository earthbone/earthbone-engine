// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! TOOL-06 logs through PRF-18's public synthetic frame and background export.
//! The five-second history and one-second tail follow docs/performance.md.

use std::time::{Duration, Instant};

use soe_trace::logging;
use soe_trace::tracing::{self, Dispatch};
use soe_trace::{FrameHistory, FrameSnapshot, Trace};

#[test]
fn tool_06_hitch_dump_contains_filtered_pending_and_worker_logs_from_its_window() {
    let directory = tempfile::tempdir().unwrap();
    let (subscriber, control) = logging::subscriber("info,papyrus=warn", None, |_| {}).unwrap();
    let dispatch = Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, || {
        Trace::set_hitch_directory(directory.path());
        Trace::set_hitch_threshold(Duration::from_millis(1)).unwrap();
        let mut history = FrameHistory::default();
        tracing::info!(target: "game", actor = "14", form_id = "0:000014", plugin = "fixture.esp", "before hitch");
        let worker = dispatch.clone();
        std::thread::spawn(move || {
            tracing::dispatcher::with_default(&worker, || {
                tracing::warn!(target: "papyrus", script = "Tool06", "worker\n\"warning\"");
                tracing::info!(target: "papyrus", "filtered message");
            });
        })
        .join()
        .unwrap();
        let frame = FrameSnapshot::now();
        std::thread::sleep(Duration::from_millis(5));
        history.record(frame.finish());
        let tail = Instant::now();
        for _ in 0..3 {
            tracing::error!(target: "game::melee", actor = "14", "weapon not equipped");
        }
        while tail.elapsed() < Duration::from_millis(1100) {
            let frame = FrameSnapshot::now();
            std::thread::sleep(Duration::from_millis(10));
            history.record(frame.finish());
        }
        Trace::wait_for_hitch_exports().unwrap();
        tracing::info!(target: "game", "after window");
    });
    // CONTEXT: No flush occurs before capture, so pending buckets must be exported.
    let path = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let logs: Vec<_> = json["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["cat"] == "log")
        .collect();
    assert!(logs
        .iter()
        .any(|event| event["args"]["fields"]["message"] == "before hitch"
            && event["args"]["fields"]["form_id"] == "0:000014"
            && event["args"]["fields"]["plugin"] == "fixture.esp"));
    assert!(logs.iter().any(
        |event| event["args"]["fields"]["message"] == "worker\n\"warning\""
            && event["args"]["fields"]["script"] == "Tool06"
    ));
    assert_eq!(
        logs.iter()
            .filter(|event| event["args"]["fields"]["message"] == "weapon not equipped")
            .map(|event| event["args"]["count"].as_u64().unwrap())
            .sum::<u64>(),
        3
    );
    assert!(logs
        .iter()
        .all(|event| event["ph"] == "i" && event["ts"].as_f64().unwrap() >= 0.0));
    let text = std::fs::read_to_string(path.with_extension("log")).unwrap();
    assert!(text.contains("before hitch") && text.contains("worker\\n\\\"warning\\\""));
    assert!(!text.contains("filtered message") && !text.contains("after window"));
    control.flush_all().unwrap();
}
