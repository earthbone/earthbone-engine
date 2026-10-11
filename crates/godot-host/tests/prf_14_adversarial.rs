// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Bound live player work counts on the installed host scenario.

const OPERATIONS: [&str; 10] = [
    "navmesh_locate",
    "navmesh_triangles",
    "path_nodes",
    "papyrus_instructions",
    "native_calls",
    "conditions",
    "raycasts",
    "shape_casts",
    "record_lookups",
    "render_instances",
];

const LIMITS: [u64; 10] = [
    2_000, 8_000, 256, 4_096, 2_048, 4_096, 64, 128, 100_000, 2_048,
];

#[test]
fn prf_14_player_operation_counts_stay_within_fixed_frame_limits() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .args(["--script", "prf_14_work_test.gd"])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot PRF-14 scenario starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS"), "{stdout}\n{stderr}");

    let report = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PRF_14_PLAYER_COUNTS "))
        .expect("player work report is present");
    let counts: serde_json::Value = serde_json::from_str(report).expect("player report is JSON");
    for (operation, limit) in OPERATIONS.into_iter().zip(LIMITS) {
        let count = counts[operation]
            .as_u64()
            .unwrap_or_else(|| panic!("missing integer count for {operation}: {counts}"));
        assert!(count <= limit, "{operation} count {count} exceeds {limit}");
    }
}
