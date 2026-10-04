// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::{path::Path, process::Command};

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
fn tool_06_breaker_system_directives_filter_subtargets_and_reload_live() {
    let (subscriber, control, lines) = captured("info,game::ai=debug,papyrus=trace");
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(target: "game::ai::combat", actor = "14", "ai detail");
        tracing::debug!(target: "game::melee", actor = "14", "hidden detail");
        control.set_filter("off,game::melee=error").unwrap();
        tracing::error!(target: "game::melee", actor = "14", "melee failure");
        tracing::error!(target: "papyrus", script = "Probe", "hidden papyrus error");
    });
    control.flush_all().unwrap();
    let lines = lines.lock().unwrap();
    assert!(lines
        .iter()
        .any(|line| line.fields.get("message").is_some_and(|v| v == "ai detail")));
    assert!(lines.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|v| v == "melee failure")));
    assert!(!lines.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|v| v == "hidden detail")));
    assert!(!lines.iter().any(|line| line
        .fields
        .get("message")
        .is_some_and(|v| v == "hidden papyrus error")));
}

#[test]
fn tool_06_breaker_rate_collapse_keeps_actor_identity_and_second_boundaries() {
    let (subscriber, control, lines) = captured("trace");
    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..3 {
            tracing::error!(target: "game::melee", system = "game::melee", actor = "14", "weapon not equipped");
        }
        tracing::error!(target: "game::melee", system = "game::melee", actor = "15", "weapon not equipped");
    });
    control.flush_all().unwrap();
    let output = lines.lock().unwrap();
    assert_eq!(output.len(), 2);
    assert!(output
        .iter()
        .any(|line| line.count == 3 && line.fields["actor"] == "14"));
    assert!(output
        .iter()
        .any(|line| line.count == 1 && line.fields["actor"] == "15"));
}

#[test]
fn tool_06_breaker_window_uses_inclusive_event_timestamps_before_grouping() {
    let (subscriber, control, _) = captured("trace");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "game::ai", marker = "boundary", "window boundary");
    });
    let event = control
        .window(0, u64::MAX)
        .into_iter()
        .find(|line| {
            line.fields
                .get("marker")
                .is_some_and(|value| value == "boundary")
        })
        .unwrap();
    let exact = control.window(event.first_ms, event.first_ms);
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].fields["marker"], "boundary");
}

#[test]
fn tool_06_breaker_godot_routes_levels_and_exposes_live_controls() {
    let Ok(binary) = std::env::var("SOE_GODOT_BIN") else {
        panic!("SOE_GODOT_BIN is required for the TOOL-06 host routing check");
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // CONTEXT: Cargo test does not refresh the release library Godot loads.
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let project = workspace.join("godot");
    for editor in [false, true] {
        let mut command = Command::new(&binary);
        command
            .env("SOE_LOG", "warn")
            .args(["--path"])
            .arg(&project)
            .args([
                "--resolution",
                "640x360",
                "--script",
                "res://tool_06_logging_test.gd",
            ]);
        if editor {
            command.args(["--editor", "--headless"]);
        }
        let output = command.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "editor={editor}: {stdout}\n{stderr}"
        );
        assert!(
            stdout.contains("tool_06_godot_routing: failures=0"),
            "{stdout}\n{stderr}"
        );
        assert!(
            stderr
                .lines()
                .any(|line| line.starts_with("ERROR:")
                    && line.contains("tool_06 weapon not equipped")),
            "{stderr}"
        );
        assert!(
            stderr
                .lines()
                .any(|line| line.starts_with("WARNING:") && line.contains("tool_06 warning")),
            "{stderr}"
        );
        assert!(stdout.contains("tool_06 info"), "{stdout}\n{stderr}");
    }
}
