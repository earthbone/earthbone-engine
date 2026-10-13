// SPDX-License-Identifier: GPL-3.0-or-later

// Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab,
// docs/research/streaming-comparison-results.md at PR 212, 97be55d), MIT License,
// Copyright (c) the Mudcrab contributors

//! Matched Bannered Mare to gate return observations follow Mudcrab PR 212.
//! The child runs ordinary collision-driven movement and exits on route completion.

use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn process_rss(root: u32) -> Option<u64> {
    let mut pending = VecDeque::from([root]);
    let mut peak = None;
    while let Some(pid) = pending.pop_front() {
        let directory = Path::new("/proc").join(pid.to_string());
        if let Ok(tasks) = fs::read_dir(directory.join("task")) {
            for task in tasks.flatten() {
                if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                    pending.extend(
                        children
                            .split_whitespace()
                            .filter_map(|child| child.parse::<u32>().ok()),
                    );
                }
            }
        }
        let is_godot = fs::read_link(directory.join("exe")).is_ok_and(|exe| {
            exe.file_name()
                .is_some_and(|name| name.to_string_lossy().to_lowercase().contains("godot"))
        });
        if is_godot {
            if let Ok(status) = fs::read_to_string(directory.join("status")) {
                for line in status
                    .lines()
                    .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
                {
                    let value = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|value| value.parse::<u64>().ok());
                    if let Some(value) = value {
                        peak = Some(peak.map_or(value, |old: u64| old.max(value)));
                    }
                }
            }
        }
    }
    peak
}

#[test]
fn mudcrab_installed_mare_gate_return_reports_hitches_coverage_and_rss() -> std::io::Result<()> {
    let (Some(godot), Some(data)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        return Ok(());
    };
    if std::env::var("DISPLAY").as_deref() != Ok(":99") {
        return Err(std::io::Error::other("streaming route requires Xvfb :99"));
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let label = std::env::var("SOE_STREAM_ROUTE_LABEL").unwrap_or_else(|_| "current".into());
    let output = root.join("target/mudcrab_streaming").join(label);
    fs::create_dir_all(&output)?;
    let mut child = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mudcrab_streaming_route.gd"))
        .env("SKYRIM_DATA", data)
        .env("DISPLAY", ":99")
        .env("SOE_GODOT_TIMEOUT", "600")
        .stdout(Stdio::from(File::create(output.join("stdout.log"))?))
        .stderr(Stdio::from(File::create(output.join("stderr.log"))?))
        .spawn()?;
    let mut peak = None;
    let status = loop {
        if let Some(rss) = process_rss(child.id()) {
            peak = Some(peak.map_or(rss, |old: u64| old.max(rss)));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let result = serde_json::json!({"exit_code": status.code(), "exit_status": status.to_string(), "peak_vmrss_kib": peak});
    fs::write(output.join("process.json"), result.to_string())?;
    let stdout = fs::read_to_string(output.join("stdout.log"))?;
    let stderr = fs::read_to_string(output.join("stderr.log"))?;
    if !status.success()
        || stderr.contains("SCRIPT ERROR")
        || !stdout.contains("STREAM_ROUTE_RESULT")
    {
        return Err(std::io::Error::other(format!(
            "streaming route failed: {result}, logs in {}",
            output.display()
        )));
    }
    Ok(())
}
