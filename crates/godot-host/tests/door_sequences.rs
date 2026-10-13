// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed door sequence binding, hinge placement and collision acceptance.
//! Thresholds: 0.01 units of hinge drift, 32 units of swing-door edge travel,
//! and 0.0002 metres of error against the authored vertex pose.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::{DataSource, LoadedCell};
use soe_godot_host::skinned::Affine;
use soe_nif::object_animation::ObjectPose;
use soe_physics::{Iso, Vec3};
use std::path::{Path, PathBuf};

#[path = "common/isolated_project.rs"]
mod isolated_project;

fn point(iso: Iso, point: Vec3) -> [f32; 3] {
    let value = iso.apply(point);
    let value = soe_godot_host::units::SkyrimPos::new(value.x, value.y, value.z).to_godot();
    [value.x, value.y, value.z]
}

fn vertices(loaded: &LoadedCell, pose: &ObjectPose) -> Vec<[f32; 3]> {
    loaded
        .surfaces
        .iter()
        .flat_map(|surface| {
            assert!(surface.animation.is_some());
            let Some(animation) = &surface.animation else {
                return Vec::new();
            };
            let transform =
                Affine::from_nif(&animation.placement.then(&pose.transforms[animation.block]))
                    .to_godot();
            surface
                .arrays
                .positions
                .iter()
                .map(|position| transform.apply(*position))
                .collect()
        })
        .collect()
}

#[test]
fn door_sequences_installed_hinges_and_collision_use_authored_child_tracks(
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return Ok(());
    };
    let source = DataSource::open(Path::new(&data))?;
    let order = source.shared_order();
    let mut manifest = Vec::new();
    for name in ["WhiterunBanneredMare", "WhiterunBreezehome"] {
        let cell = source.resolve_cell(name).ok_or("missing vanilla cell")?;
        for &id in source.cell_index().references(cell) {
            let Some(owner) = order.winner_ref(id) else {
                continue;
            };
            let record = order.record(owner);
            if record.tag != Tag(*b"REFR") {
                continue;
            }
            let reference = Refr::from_record(record)?;
            let Some(base) = order.winner(order.resolve(owner.file, reference.base)) else {
                continue;
            };
            if base.tag != Tag(*b"DOOR")
                || !soe_game::World::authored_reference_enabled(&order, id)
                || base
                    .editor_id()
                    .is_some_and(|name| name.ends_with("MinUse"))
            {
                continue;
            }
            let loaded = source.stream_reference(id)?;
            let animation = loaded
                .surfaces
                .first()
                .and_then(|s| s.animation.as_ref())
                .ok_or("door lost controller asset")?;
            let asset = &animation.asset;
            let duration = asset
                .sequence_duration("Open")
                .ok_or("door has no Open sequence")?;
            let close_duration = asset
                .sequence_duration("Close")
                .ok_or("door has no Close sequence")?;
            assert!(duration > 0.0 && close_duration > 0.0);
            let closed = asset.sample(0.0, Some("Open"));
            let open = asset.sample(duration, Some("Open"));
            let colliders = source.placed_reference_colliders(id)?;
            assert!(!colliders.is_empty());
            for placed in &colliders {
                let node = placed.collider.node.get().ok_or("collision has no node")?;
                assert!(
                    (Vec3::from(closed.transforms[node].translation)
                        - Vec3::from(open.transforms[node].translation))
                    .length()
                        < 0.01,
                    "offset child hinge stays fixed"
                );
                assert_ne!(
                    closed.transforms[node].rotation, open.transforms[node].rotation,
                    "collision node rotates about its hinge"
                );
            }
            assert_ne!(closed, open, "Open changes the child pose");
            assert_eq!(
                closed.transforms[0], open.transforms[0],
                "model root stays fixed"
            );
            let closed_vertices = vertices(&loaded, &closed);
            let open_vertices = vertices(&loaded, &open);
            let max_travel = closed_vertices
                .iter()
                .zip(&open_vertices)
                .map(|(a, b)| {
                    (0..3)
                        .map(|axis| (a[axis] - b[axis]).powi(2))
                        .sum::<f32>()
                        .sqrt()
                })
                .fold(0.0, f32::max);
            if reference.teleport.is_none() {
                assert!(
                    max_travel > 32.0 * soe_godot_host::units::METRES_PER_UNIT,
                    "free edge moves at least 32 units"
                );
            }
            let mut samples = Vec::new();
            for (sequence, time) in [
                ("Open", 0.0),
                ("Open", duration * 0.5),
                ("Open", duration),
                ("Close", close_duration * 0.5),
                ("Close", close_duration),
            ] {
                let pose = asset.sample(time, Some(sequence));
                let mut hinges = Vec::new();
                let mut rays = Vec::new();
                for placed in &colliders {
                    let collider = &placed.collider;
                    let node = collider.node.get().ok_or("missing collision node")?;
                    let old = placed
                        .original
                        .then(&Iso::from(asset.rest_pose().transforms[node]));
                    let new = placed.original.then(&Iso::from(pose.transforms[node]));
                    let delta = new.then(&old.inverse());
                    let transform = delta.then(&collider.transform);
                    let bounds = collider.shape.aabb(&Iso::IDENTITY);
                    let center = (bounds.min + bounds.max) * 0.5;
                    let size = bounds.max - bounds.min;
                    let normal = if size.x < size.y {
                        Vec3::new(1.0, 0.0, 0.0)
                    } else {
                        Vec3::new(0.0, 1.0, 0.0)
                    };
                    rays.push(serde_json::json!([
                        point(transform, center - normal * 16.0),
                        point(transform, center + normal * 16.0)
                    ]));
                    hinges.push(serde_json::json!({"node":node,"position":point(new, Vec3::ZERO)}));
                }
                samples.push(serde_json::json!({"sequence":sequence,"time":time,"vertices":vertices(&loaded, &pose),"hinges":hinges,"rays":rays}));
            }
            println!("door {name} {id} {:?}: Open {duration}s Close {close_duration}s edge {max_travel}m", base.editor_id());
            manifest.push(
                serde_json::json!({"cell":name,"file":id.file,"object":id.object,
                "position":reference.transform.ok_or("door placement missing")?.position,
                "teleport":reference.teleport.is_some(),"samples":samples}),
            );
        }
    }
    assert_eq!(
        manifest.len(),
        6,
        "five Bannered Mare doors and one Breezehome door"
    );
    let godot =
        std::env::var_os("SOE_GODOT_BIN").ok_or("installed scene requires SOE_GODOT_BIN")?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&target)?;
    std::fs::write(
        target.join("door_sequences.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    let target = target.canonicalize()?;
    let project = isolated_project::project(
        &["door_sequences_test.gd"],
        Some(&target.join("release/libsoe_godot_host.so")),
    );
    let output = std::process::Command::new(godot)
        .env("CARGO_TARGET_DIR", &target)
        .arg("--path")
        .arg(project.path())
        .args([
            "--resolution",
            "640x360",
            "--script",
            "door_sequences_test.gd",
        ])
        .output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(target.join("door_sequences.log"), &log)?;
    assert!(
        output.status.success() && log.contains("RESULT: PASS door_sequences"),
        "{log}"
    );
    println!("{log}");
    Ok(())
}
