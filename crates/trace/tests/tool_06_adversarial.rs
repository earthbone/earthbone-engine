// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use soe_trace::logging::{self, LogLine};
use soe_trace::tracing::{self, Subscriber};

fn captured(
    filter: &str,
) -> (
    impl Subscriber + Send + Sync,
    logging::LogControl,
    Arc<Mutex<Vec<LogLine>>>,
) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let (subscriber, control) = logging::subscriber(filter, None, move |line| {
        sink.lock().unwrap().push(line.clone());
    })
    .unwrap();
    (subscriber, control, lines)
}

#[test]
fn tool_06_adversarial_system_filter_fields_and_rate_window() {
    let (subscriber, control, lines) = captured("info,game::ai=debug,papyrus=trace");
    let start = control.now_ms();
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(target: "game", system = "game", message = "hidden global debug");
        tracing::debug!(target: "game::ai", system = "game::ai", actor = "14", message = "ai detail");
        tracing::trace!(target: "papyrus", system = "papyrus", script = "Fixture", message = "vm detail");
        for _ in 0..3 {
            tracing::error!(target: "game::melee", system = "game::melee", actor = "14", message = "weapon not equipped");
        }
        tracing::error!(target: "game::melee", system = "game::melee", actor = "15", message = "weapon not equipped");
    });

    let end = control.now_ms();
    let window = control.window(start, end);
    let collapsed = window.iter().find(|line| {
        line.fields
            .get("message")
            .is_some_and(|m| m == "weapon not equipped")
    });
    assert_eq!(collapsed.map(|line| line.count), Some(3));
    assert!(window
        .iter()
        .any(|line| line.fields.get("actor").is_some_and(|actor| actor == "15")));
    assert!(window
        .iter()
        .any(|line| line.fields.get("message").is_some_and(|m| m == "ai detail")));
    assert!(window
        .iter()
        .any(|line| line.fields.get("message").is_some_and(|m| m == "vm detail")));
    assert!(!window.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|m| m == "hidden global debug")));

    std::thread::sleep(Duration::from_millis(1_010));
    control.flush().unwrap();
    let output = lines.lock().unwrap();
    assert!(output.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|m| m == "weapon not equipped")
        && line.count == 3));
    assert!(output.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|m| m == "weapon not equipped")
        && line.fields.get("actor").is_some_and(|actor| actor == "15")));
}
