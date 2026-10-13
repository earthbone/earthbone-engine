// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed inn and gate memory attribution using the production walkthrough.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn installed_memory_attribution() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return Ok(());
    }
    let godot = std::env::var_os("SOE_GODOT_BIN").ok_or("SOE_GODOT_BIN required")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output_dir = std::env::var_os("SOE_RSS_OUTPUT")
        .map_or_else(|| root.join("target/memory"), PathBuf::from);
    std::fs::create_dir_all(&output_dir)?;
    let output = Command::new(godot)
        .args(["--path"])
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "installed_memory_attribution.gd",
        ])
        .env("SOE_MEMORY_REPORT", output_dir.join("attribution.json"))
        .env("SOE_THREADED_SIMULATION", "0")
        .output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(output_dir.join("attribution.log"), &log)?;
    assert!(
        output.status.success()
            && log.contains("installed_memory_attribution: PASS")
            && !log.contains("SCRIPT ERROR:"),
        "memory capture exited {}, see {}/attribution.log",
        output.status,
        output_dir.display()
    );
    Ok(())
}
