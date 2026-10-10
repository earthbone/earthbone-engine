// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-29 installed plains coverage and unload restoration in a windowed host.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_29_installed_plains_capture_has_no_overlap_or_boundary_gap() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed capture");
        return;
    };
    let godot =
        std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN required for installed capture");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    assert!(
        workspace
            .join("target/release/libsoe_godot_host.so")
            .is_file(),
        "build the release extension before the capture"
    );
    let eye_height = {
        let source = soe_godot_host::data_source::DataSource::open(&PathBuf::from(&data)).unwrap();
        let order = source.shared_order();
        let cell = source.resolve_cell("WhiterunWatchtowerExterior").unwrap();
        let height = order
            .winners_of(soe_esm::tag::LAND)
            .find_map(|(id, record)| {
                let owner = order.winner_ref(id)?;
                let raw_cell = record.placement.cell?;
                if order.resolve(owner.file, raw_cell) != cell {
                    return None;
                }
                let land = soe_esm::records::land::Land::from_record(record).unwrap();
                Some(land.heights.unwrap().decode()[16 * 33 + 16])
            })
            .expect("plains centre has LAND");
        height * soe_godot_host::units::METRES_PER_UNIT + 1.7
    };
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(project.join("rnd_29_terrain_join_capture.gd"))
        .env("SOE_RND29_EYE_HEIGHT", eye_height.to_string())
        .env("SKYRIM_DATA", data)
        .output()
        .expect("start Godot");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = project.join(".godot/rnd-29-terrain-join.log");
    std::fs::write(&log, format!("{stdout}\n{stderr}")).unwrap();
    assert!(
        output.status.success()
            && stdout
                .contains("RESULT: PASS: RND-29 terrain joins loaded cells without overlap or gap"),
        "capture failed, log {}\n{stdout}\n{stderr}",
        log.display()
    );
    assert!(project.join(".godot/rnd-29-plains.png").is_file());
    assert!(project.join(".godot/rnd-29-coverage.png").is_file());
    assert!(project.join(".godot/rnd-29-standing-plains.png").is_file());
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("RND-29") || line.starts_with("RESULT:"))
    {
        println!("{line}");
    }
}
