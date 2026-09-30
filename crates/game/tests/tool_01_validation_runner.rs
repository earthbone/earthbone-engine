// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("soe-tool-01-{}-{stamp}", std::process::id()))
}

#[test]
fn tool_01_runner_loads_profile_runs_scenarios_and_prints_cited_capture() {
    let root = scratch_dir();
    let game = root.join("game");
    let mo2 = root.join("mo2");
    let profile = mo2.join("profiles/Default");
    let scenarios = root.join("scenarios/group");
    fs::create_dir_all(game.join("Data")).unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::create_dir_all(scenarios.join("captures")).unwrap();
    fs::write(profile.join("plugins.txt"), "").unwrap();
    fs::write(profile.join("loadorder.txt"), "").unwrap();
    fs::write(profile.join("modlist.txt"), "").unwrap();
    fs::write(scenarios.join("captures/oracle.log"), "").unwrap();
    fs::write(
        scenarios.join("clock.scenario"),
        "# preserve: scenario checks run against the selected profile\ntick 1\nexpect traces-match captures/oracle.log\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_soe-validate"))
        .arg(&game)
        .arg(&mo2)
        .arg("Default")
        .arg(root.join("scenarios"))
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Conformance report: 1 scenario(s)"));
    assert!(stdout.contains("Overall: PASS"));
    assert!(
        stdout.contains("PASS clock [preserve: scenario checks run against the selected profile]")
    );
    assert!(stdout.contains(&scenarios.join("captures/oracle.log").display().to_string()));

    fs::remove_dir_all(root).unwrap();
}
