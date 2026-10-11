// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Runs the installed-data CMB-27 hit through the live Godot host and actor graph.

use std::{
    path::PathBuf,
    process::{Child, Command, Output},
    thread,
    time::{Duration, Instant},
};

fn wait_for_exit(mut child: Child, timeout: Duration) -> Option<Output> {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .expect("Godot process status is readable")
            .is_some()
        {
            return Some(child.wait_with_output().expect("Godot output is readable"));
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .expect("timed out Godot process can be stopped");
            let _ = child.wait_with_output();
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn cmb_27_live_godot_hit_plays_reaction_returns_and_damages_once() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA is unset, skipping CMB-27 live Godot check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN is unset, skipping CMB-27 live Godot check");
        return;
    };

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"))
        .join("release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("release extension is missing, skipping CMB-27 live Godot check");
        return;
    }

    let child = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg("cmb_27_live_reaction_test.gd")
        .env("SKYRIM_DATA", data)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Godot starts for the CMB-27 live hit check");

    let Some(output) = wait_for_exit(child, Duration::from_secs(180)) else {
        panic!("CMB-27 live Godot check exceeded 180 seconds");
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "CMB-27 live Godot hit check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
