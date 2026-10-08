// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-08 loading and cold-start reporting, from Godot's pipeline compilation
//! guide. Headless checks exercise contracts, windowed checks measure the host.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn godot_binary() -> Option<PathBuf> {
    if let Some(binary) =
        std::env::var_os("SOE_GODOT_BIN").or_else(|| std::env::var_os("SOE_GODOT"))
    {
        return Some(binary.into());
    }
    Command::new("godot")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| "godot".into())
}

#[test]
fn prf_08_loading_reporting_and_repetition_contracts_in_stock_godot() {
    let Some(godot) = godot_binary() else {
        eprintln!("SOE_GODOT not set and godot not installed, skipping GDScript contracts");
        return;
    };
    let root = workspace();
    let project = tempfile::tempdir_in(&root).unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"PRF-08 contracts\"\n",
    )
    .unwrap();
    for script in [
        "render_warmup.gd",
        "bench_metrics.gd",
        "bench_runs.gd",
        "prf_08_test.gd",
        "bench.gd",
        "system_overlay.gd",
        "transition_metrics.gd",
        "walkthrough.gd",
        "record_sky.gd",
        "record_sky.gdshader",
        "record_fog.gdshader",
    ] {
        std::fs::copy(root.join("godot").join(script), project.path().join(script)).unwrap();
    }
    for script in ["prf_08_test.gd", "bench.gd", "walkthrough.gd"] {
        let mut command = Command::new(&godot);
        command
            .args(["--headless", "--path"])
            .arg(project.path())
            .args(["--script", script])
            .env("XDG_DATA_HOME", project.path());
        if script != "prf_08_test.gd" {
            command.arg("--check-only");
        }
        let output = command.output().unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success(), "{script}: {log}");
        assert!(!log.contains("SCRIPT ERROR:"), "{script}: {log}");
        if script == "prf_08_test.gd" {
            assert!(log.contains("RESULT: PASS: prf_08"), "{log}");
        }
    }
}

#[test]
#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
fn prf_08_whiterun_mesh_materials_are_bound_before_first_frame() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        eprintln!("SKYRIM_DATA not set, skipping installed mesh material check");
        return;
    }
    let Some(godot) = godot_binary() else {
        eprintln!("SOE_GODOT not set and godot not installed, skipping host material check");
        return;
    };
    let root = workspace();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let library = target.join("debug/deps/libsoe_godot_host.so");
    assert!(
        library.is_file(),
        "cargo test must build {}",
        library.display()
    );
    let project = tempfile::tempdir_in(&root).unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"PRF-08 materials\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("soe.gdextension"),
        format!(
            "[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\n",
            library.display()
        ),
    )
    .unwrap();
    std::fs::create_dir(project.path().join(".godot")).unwrap();
    std::fs::write(
        project.path().join(".godot/extension_list.cfg"),
        "res://soe.gdextension\n",
    )
    .unwrap();
    std::fs::copy(
        root.join("godot/prf_08_material_test.gd"),
        project.path().join("prf_08_material_test.gd"),
    )
    .unwrap();
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "prf_08_material_test.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "{log}");
    assert!(
        !log.contains("SCRIPT ERROR:") && !log.contains("ERROR:"),
        "{log}"
    );
    assert!(log.contains("RESULT: PASS: prf_08"), "{log}");
}

#[test]
#[cfg(feature = "godot")]
fn prf_08_three_windowed_cold_whiterun_runs_under_33_ms() {
    if std::env::var_os("SKYRIM_DATA").is_none()
        || std::env::var("SOE_PRF_08_WINDOWED").as_deref() != Ok("1")
    {
        eprintln!("SKYRIM_DATA and SOE_PRF_08_WINDOWED=1 required, skipping windowed acceptance");
        return;
    }
    let godot = godot_binary().expect("windowed acceptance requires SOE_GODOT or godot");
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace().join("godot"))
        .args(["--script", "bench.gd", "--", "--cold-starts", "3"])
        .env_remove("SOE_BENCH_SOAK")
        .env_remove("SOE_BENCH_CAPTURE")
        .env("SOE_BENCH_SECONDS", "5")
        .env("SOE_BENCH_P99_MS", "16.7")
        .env("SOE_BENCH_WORST_MS", "33")
        .env("SOE_BENCH_CELL", "WhiterunOrigin")
        .env("SOE_BENCH_RADIUS", "1")
        .env("SOE_BENCH_BATCH", "1")
        .env("SOE_BENCH_OCCLUDERS", "1")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "{log}");
    assert!(
        !log.contains("SCRIPT ERROR:") && !log.contains("ERROR:"),
        "{log}"
    );
    let reports: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("godot_run "))
        .collect();
    assert_eq!(reports.len(), 3, "three fresh children must report: {log}");
    for (index, report) in reports.iter().enumerate() {
        let fields: Vec<_> = report.split_whitespace().collect();
        assert_eq!(fields[1].parse::<usize>().unwrap(), index + 1);
        assert!(fields[3].parse::<f64>().unwrap() < 33.0, "{report}");
        assert!(fields[6].parse::<u64>().unwrap() > 0, "{report}");
    }
    assert!(log.contains("godot_cold_starts 3 runs exit 0"), "{log}");
}
