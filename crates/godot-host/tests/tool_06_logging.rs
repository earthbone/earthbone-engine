// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Live GlobalScope routing and UI controls on Godot 4.4 or later.
//! Build/import the release extension before running with SOE_GODOT_BIN set.

use std::path::Path;
use std::process::Command;

#[test]
fn tool_06_godot_routing_and_editor_debugger() {
    let Ok(binary) = std::env::var("SOE_GODOT_BIN") else {
        return;
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
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
            "errors must reach Godot push_error: {stderr}"
        );
        assert!(
            stderr
                .lines()
                .any(|line| line.starts_with("WARNING:") && line.contains("tool_06 warning")),
            "warnings must reach Godot push_warning: {stderr}"
        );
        assert!(
            stdout.contains("tool_06 info"),
            "info must reach Godot output: {stdout}"
        );
        assert!(!stdout.contains("tool_06 debug") && !stdout.contains("tool_06 trace"));
    }
}
