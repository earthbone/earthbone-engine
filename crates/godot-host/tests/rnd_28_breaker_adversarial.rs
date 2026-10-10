// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for xEdit's Skyrim LST and BTT readers.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_28_lst_accepts_bytes_after_the_declared_table() {
    // xEdit wbLOD.pas checks that entries fit, then copies the declared table.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend(7u32.to_le_bytes());
    for value in [100f32, 200.0, 0.25, 0.125, 0.75, 0.875] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0u32.to_le_bytes());
    bytes.extend([0xde, 0xad, 0xbe, 0xef]);

    let parsed = soe_godot_host::tree_lod::read_types(&bytes);
    assert!(
        parsed.is_ok(),
        "xEdit accepts trailing bytes, got {parsed:?}"
    );
    let parsed = parsed.unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[&7].width, 100.0);
}

#[test]
fn rnd_28_player_capture_streams_mountains_trees_and_world_changes() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN");
    assert!(workspace
        .join("target/release/libsoe_godot_host.so")
        .is_file());

    let source = soe_godot_host::data_source::DataSource::open(&data).unwrap();
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

    let (solstheim, _) = source.resolve_world("DLC2SolstheimWorld").unwrap();
    let order = source.shared_order();
    let cell = order
        .winners_of(soe_esm::tag::CELL)
        .find_map(|(id, record)| {
            let grid = source.cell_index().grid_of(id)?;
            (grid.world == solstheim
                && record
                    .editor_id()
                    .is_some_and(|name| name.starts_with("DLC2RavenRock")))
            .then_some(id)
        })
        .unwrap();
    let reference = source
        .cell_index()
        .references(cell)
        .iter()
        .find(|id| {
            order.winner(**id).is_some_and(|record| {
                record.tag == soe_esm::tag::REFR
                    && !record.is_deleted()
                    && record.flags & soe_esm::plugin::flags::INITIALLY_DISABLED == 0
                    && soe_esm::records::refr::Refr::from_record(record)
                        .is_ok_and(|refr| refr.transform.is_some() && refr.enable_parent.is_none())
            })
        })
        .unwrap();
    let other_reference = format!("{}:{}", reference.file, reference.object);
    drop(source);

    let capture = project.join(".godot/rnd-28-breaker.png");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_28_player_lod_capture.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_RND28_OTHER_REFERENCE", other_reference)
        .env("SOE_RND28_MOUNTAIN_TARGET", mountain_target)
        .env("SOE_RND28_CAPTURE_PATH", &capture)
        .output()
        .expect("Godot capture starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let diagnostics = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| {
            line.contains("FAIL:")
                || line.contains("RESULT:")
                || line.contains("RND-28")
                || line.contains("SCRIPT ERROR:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout
                .contains("RESULT: PASS: PlayerNode streams object and tree LOD around Whiterun"),
        "RND-28 player capture failed: {diagnostics}"
    );
    assert!(capture.is_file(), "capture output missing");
}
