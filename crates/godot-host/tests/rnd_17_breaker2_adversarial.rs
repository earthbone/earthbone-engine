// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 installed barricade models must yield their intact enabled geometry.

use soe_esm::tag;
use soe_game::World;
use soe_godot_host::data_source::DataSource;
use std::path::Path;
use std::process::Command;

#[test]
fn rnd_17_all_whiterun_barricades_build_geometry_and_collision_when_enabled() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let (whiterun, _) = source.resolve_world("WhiterunWorld").unwrap();
    let mut candidates = Vec::new();
    for (id, record) in order.winners_of(tag::REFR) {
        let Some(owner) = order.winner_ref(id) else {
            continue;
        };
        let Some(cell) = record
            .placement
            .cell
            .map(|raw| order.resolve(owner.file, raw))
        else {
            continue;
        };
        let Some(cell_owner) = order.winner_ref(cell) else {
            continue;
        };
        let cell_record = order.record(cell_owner);
        if cell_record
            .placement
            .world
            .map(|raw| order.resolve(cell_owner.file, raw))
            != Some(whiterun)
        {
            continue;
        }
        let Some(parent_data) = record
            .find(soe_esm::Tag::new(b"XESP"))
            .filter(|bytes| bytes.len() >= 8 && bytes[4] & 1 == 0)
        else {
            continue;
        };
        let Some(base_raw) = record.find(soe_esm::Tag::new(b"NAME")) else {
            continue;
        };
        let base = order.resolve(
            owner.file,
            u32::from_le_bytes(base_raw[..4].try_into().unwrap()),
        );
        let Some(base_record) = order.winner(base) else {
            continue;
        };
        if base_record.tag != soe_esm::Tag::new(b"ACTI")
            || !base_record
                .editor_id()
                .is_some_and(|name| name.to_ascii_lowercase().contains("barricade"))
        {
            continue;
        }
        let marker = order.resolve(
            owner.file,
            u32::from_le_bytes(parent_data[..4].try_into().unwrap()),
        );
        if order.winner(marker).is_some_and(|record| {
            record.flags & soe_esm::plugin::flags::INITIALLY_DISABLED != 0
                && record.find(soe_esm::Tag::new(b"XESP")).is_none()
        }) {
            candidates.push((id, marker, cell));
        }
    }
    candidates.sort_unstable();
    assert!(candidates.len() >= 9, "survey all nine barricades");

    let mut world = World::from_shared_order(order.clone());
    for &(reference, marker, _) in &candidates {
        world.set_reference_enabled(marker, true);
        assert!(world.reference_enabled(reference));
        let loaded = source.stream_reference(reference).unwrap();
        assert!(
            !loaded.surfaces.is_empty(),
            "enabled barricade {reference} has no rendered geometry"
        );
        let colliders = source.placed_reference_colliders(reference).unwrap();
        assert!(
            colliders.iter().any(|placed| placed.collider.is_static()),
            "enabled barricade {reference} has no static collision"
        );
    }

    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let (reference, marker, cell) = candidates[0];
    let reference_record = order.winner(reference).unwrap();
    let placement = reference_record.find(soe_esm::Tag::new(b"DATA")).unwrap();
    let cell_record = order.winner(cell).unwrap();
    let coordinates = cell_record.find(soe_esm::Tag::new(b"XCLC")).unwrap();
    let read_u32 = |bytes: &[u8], offset: usize| {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
        });
    std::fs::create_dir_all(&target).unwrap();
    let manifest = format!(
        "{{\"reference\":{},\"marker\":{},\"file\":{},\"cell\":\"{}\",\"x\":{},\"y\":{},\"px\":{},\"py\":{},\"pz\":{}}}",
        reference.object,
        marker.object,
        reference.file,
        cell_record.editor_id().unwrap(),
        read_u32(coordinates, 0) as i32,
        read_u32(coordinates, 4) as i32,
        f32::from_bits(read_u32(placement, 0)),
        f32::from_bits(read_u32(placement, 4)),
        f32::from_bits(read_u32(placement, 8)),
    );
    std::fs::write(target.join("rnd_17_whiterun.json"), manifest).unwrap();
    let project = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_17_enable_parent_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run the live no-reload enable-parent check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS rnd_17_whiterun_runtime_enable_without_reload"),
        "live enable-parent check failed\n{stdout}\n{stderr}"
    );
}
