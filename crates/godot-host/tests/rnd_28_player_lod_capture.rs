// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the RND-28 gameplay-host exterior LOD capture on installed Skyrim data.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_28_player_streams_every_installed_level_and_captures_mountain_silhouette() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping the installed RND-28 capture check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping the Godot RND-28 capture check");
        return;
    };
    assert!(
        extension.exists(),
        "release extension missing at {}, build soe-godot-host first",
        extension.display()
    );

    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let (tamriel, _) = source.resolve_world("Tamriel").unwrap();
    let mountain = source
        .load_object_lod_block(
            tamriel,
            "Tamriel",
            soe_godot_host::terrain_lod::LodBlockCoord {
                level: soe_godot_host::terrain_lod::LodLevel::L16,
                x: 0,
                y: -16,
            },
        )
        .unwrap()
        .unwrap();
    let peak = mountain
        .surfaces
        .iter()
        .flat_map(|surface| surface.arrays.positions.iter())
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .unwrap();
    let peak = soe_godot_host::units::GodotPos::new(peak[0], peak[1], peak[2]).to_skyrim();
    let mountain_target = format!("{},{},{}", peak.x, peak.y, peak.z);
    drop(mountain);
    let (other_world, _) = source.resolve_world("DLC2SolstheimWorld").unwrap();
    let order = source.shared_order();
    let cell = order
        .winners_of(soe_esm::tag::CELL)
        .find_map(|(id, record)| {
            let grid = source.cell_index().grid_of(id)?;
            (grid.world == other_world
                && record
                    .editor_id()
                    .is_some_and(|name| name.starts_with("DLC2RavenRock")))
            .then_some(id)
        })
        .unwrap();
    let other_reference = source
        .cell_index()
        .references(cell)
        .iter()
        .find(|id| {
            order.winner(**id).is_some_and(|record| {
                record.tag == soe_esm::tag::REFR
                    && !record.is_deleted()
                    && record.flags & soe_esm::plugin::flags::INITIALLY_DISABLED == 0
                    && soe_esm::records::refr::Refr::from_record(record).is_ok_and(|reference| {
                        reference.transform.is_some() && reference.enable_parent.is_none()
                    })
            })
        })
        .unwrap();
    let other_reference = format!("{}:{}", other_reference.file, other_reference.object);
    drop(source);
    let capture = project.join(".godot/rnd-28-whiterun-exterior-lod.png");
    let script = project.join("rnd_28_player_lod_capture.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .env("SOE_RND28_OTHER_REFERENCE", other_reference)
        .env("SOE_RND28_MOUNTAIN_TARGET", mountain_target)
        .env("SOE_RND28_CAPTURE_PATH", &capture)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = project.join(".godot/rnd-28-player-lod.log");
    std::fs::write(&log, format!("{stdout}\n{stderr}")).expect("capture log writes");
    let diagnostics = stderr
        .lines()
        .filter(|line| line.starts_with("FAIL:") || line.starts_with("SCRIPT ERROR:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout
                .contains("RESULT: PASS: PlayerNode streams object and tree LOD around Whiterun"),
        "Godot RND-28 capture failed, full log at {}\n{diagnostics}",
        log.display()
    );
    assert!(
        capture.is_file(),
        "Godot did not save {}",
        capture.display()
    );
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("RND-28") || line.starts_with("RESULT:"))
    {
        println!("{line}");
    }
}
