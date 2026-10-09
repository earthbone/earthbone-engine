// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Installed-data gameplay frame gates through the real walkthrough startup.
//! Uses the wall-clock contract in docs/performance.md, not Godot's process monitor.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static GODOT_RUN: Mutex<()> = Mutex::new(());
const P99_MS: f64 = 16.7;
const WORST_MS: f64 = 33.0;

fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).expect("read child output");
        bytes
    })
}

fn check_sample(row: &serde_json::Value, cell: &str) {
    assert_eq!(row["cell"], cell);
    assert_eq!(row["headless"], true);
    let frames = row["samples"].as_u64().expect("frame count");
    assert!(frames >= 3600, "{row}");
    assert!(
        row["elapsed_s"].as_f64().expect("wall seconds") >= 60.0,
        "{row}"
    );
    assert!(
        row["simulated_s"].as_f64().expect("simulation seconds") >= 60.0,
        "{row}"
    );
    assert_eq!(row["settle_s"], 20.0, "{row}");
    assert_eq!(row["cell_violations"], 0, "{row}");
    let navigation_frames = frames.div_ceil(60);
    assert_eq!(row["navigation_frames"], navigation_frames, "{row}");
    assert_eq!(
        row["navigation_expected_frames"], navigation_frames,
        "{row}"
    );
    assert!(
        row["navigation_queries"]
            .as_u64()
            .expect("navigation queries")
            >= navigation_frames,
        "{row}"
    );
    let actors = row["actors"].as_u64().expect("actor count");
    assert!(actors >= 5, "{row}");
    assert!(
        row["selected_packages"]
            .as_u64()
            .expect("selected packages")
            > 0,
        "{row}"
    );
    assert!(
        row["navmesh_triangles"].as_u64().expect("NAVM triangles") > 0,
        "{row}"
    );
    assert!(
        row["evaluations"].as_u64().expect("package evaluations") >= actors,
        "{row}"
    );
    assert_eq!(
        row["selected_frames"], frames,
        "NPC packages must remain active: {row}"
    );
    assert_eq!(row["over_budget_frames"], 0, "{row}");
    let p99 = row["p99_ms"].as_f64().expect("numeric p99");
    let worst = row["worst_ms"].as_f64().expect("numeric worst frame");
    assert!(p99.is_finite() && p99 > 0.0 && p99 <= P99_MS, "{row}");
    assert!(
        worst.is_finite() && worst >= p99 && worst <= WORST_MS,
        "{row}"
    );
}

fn walkthrough(cell: &str) {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP TST-18 {cell}: SKYRIM_DATA is unset");
        return;
    };
    let _run = GODOT_RUN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN supplies Godot 4.4");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new(godot)
        .current_dir(&root)
        .args([
            "--headless",
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "tst_18_walkthrough.gd",
            "--",
            cell,
        ])
        .env("SKYRIM_DATA", data)
        .env_remove("SOE_CAPTURE_PATH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch walkthrough check");
    let stdout = drain(child.stdout.take().expect("stdout pipe"));
    let stderr = drain(child.stderr.take().expect("stderr pipe"));
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("read child status") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("reap Godot process");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().expect("stdout reader");
    let stderr = stderr.join().expect("stderr reader");
    let stdout = String::from_utf8_lossy(&stdout);
    let stderr = String::from_utf8_lossy(&stderr);
    let logs = root.join("performance-results/tst-18");
    std::fs::create_dir_all(&logs).expect("create performance log directory");
    std::fs::write(
        logs.join(format!("{cell}-{}.log", std::process::id())),
        format!("status={status} timed_out={timed_out}\nstdout:\n{stdout}\nstderr:\n{stderr}"),
    )
    .expect("retain gameplay measurements");
    assert!(
        !timed_out
            && status.success()
            && stdout.contains("RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR")
            && !stderr.contains("TST-18 FAIL"),
        "{cell}: status={status}, timed_out={timed_out}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let rows: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("TST_18_SAMPLE "))
        .collect();
    assert_eq!(rows.len(), 1, "expected one complete measurement: {stdout}");
    let row = serde_json::from_str(rows[0]).expect("valid gameplay report");
    check_sample(&row, cell);
    eprintln!("TST-18 {row}");
}

#[test]
fn tst_18_installed_bannered_mare_walkthrough_meets_every_frame_budget_for_sixty_seconds() {
    walkthrough("WhiterunBanneredMare");
}

#[test]
fn tst_18_installed_whiterun_exterior_walkthrough_meets_every_frame_budget_for_sixty_seconds() {
    walkthrough("WhiterunOrigin");
}
