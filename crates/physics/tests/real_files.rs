// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every collision block in a sample of the shipped meshes, read straight
//! out of the archive through `soe_bsa`. Gated on `SKYRIM_DATA` pointing
//! at the game's `Data` folder. `SOE_PHYSICS_LIMIT` caps the number of
//! meshes (default 2000, 0 for all).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use soe_nif::blocks::Block;
use soe_physics::bhk::{self, Bhk};
use soe_physics::{collect, PhysicsWorld, Shape, SimpleWorld};

fn count_shapes(s: &Shape, tally: &mut BTreeMap<&'static str, usize>) {
    let key = match s {
        Shape::Box { .. } => "box",
        Shape::Sphere { .. } => "sphere",
        Shape::Capsule { .. } => "capsule",
        Shape::ConvexHull { .. } => "convex hull",
        Shape::TriangleMesh { .. } => "triangle mesh",
        Shape::Compound(children) => {
            for (_, c) in children {
                count_shapes(c, tally);
            }
            "compound"
        }
    };
    *tally.entry(key).or_default() += 1;
}

#[test]
fn shipped_meshes_decode_to_colliders() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let limit: usize = std::env::var("SOE_PHYSICS_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let started = Instant::now();
    let mut files = 0usize;
    let mut with_colliders = 0usize;
    let mut colliders = 0usize;
    let mut triangles = 0usize;
    let mut parse_failures = Vec::new();
    let mut shapes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut block_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut bad_bodies = Vec::new();
    let mut huge = Vec::new();
    let mut motions: BTreeMap<String, usize> = BTreeMap::new();
    let mut layers: BTreeMap<u8, usize> = BTreeMap::new();
    let mut world = SimpleWorld::new();
    for archive_name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(archive_name)).expect("archive opens");
        let names: Vec<String> = archive
            .files()
            .iter()
            .filter(|n| n.to_lowercase().ends_with(".nif"))
            .cloned()
            .collect();
        for name in names {
            if limit != 0 && files >= limit {
                break;
            }
            let bytes = archive.read(&name).expect("file reads");
            files += 1;
            let file = match soe_nif::parse(&bytes) {
                Ok(f) => f,
                Err(e) => {
                    parse_failures.push(format!("{name}: {e}"));
                    continue;
                }
            };
            for b in &file.blocks {
                if let Block::Collision { type_name, data } = b {
                    *block_types.entry(type_name.clone()).or_default() += 1;
                    if type_name == "bhkRigidBodyT" {
                        match bhk::decode(type_name, data, &file.header) {
                            Ok(Bhk::RigidBody(rb)) => {
                                let n = rb.rotation.norm();
                                if !(0.99..=1.01).contains(&n) || !rb.translation.is_finite() {
                                    bad_bodies
                                        .push(format!("{name}: norm {n} {:?}", rb.translation));
                                }
                            }
                            other => bad_bodies.push(format!("{name}: {other:?}")),
                        }
                    }
                }
            }
            let c = match collect(&file) {
                Ok(c) => c,
                Err(e) => {
                    bad_bodies.push(format!("{name}: collect failed: {e}"));
                    continue;
                }
            };
            for (_, why) in &c.skipped {
                *skipped.entry(why.clone()).or_default() += 1;
            }
            if !c.colliders.is_empty() {
                with_colliders += 1;
            }
            for col in &c.colliders {
                colliders += 1;
                triangles += col.shape.triangle_count();
                count_shapes(&col.shape, &mut shapes);
                *motions.entry(format!("{:?}", col.motion)).or_default() += 1;
                *layers.entry(col.layer.0).or_default() += 1;
                let bb = col.aabb();
                let extent = (bb.max - bb.min).max_element();
                if !extent.is_finite() || extent > 200_000.0 {
                    huge.push(format!("{name}: {} extent {extent}", col.node_name));
                }
                if col.is_static() {
                    world.add_static(col);
                }
            }
        }
    }
    world.step(1.0 / 60.0);
    eprintln!(
        "{files} meshes in {:.1?}: {with_colliders} with collision, {colliders} colliders, {triangles} triangles, world holds {} statics / {} primitives",
        started.elapsed(),
        world.static_count(),
        world.primitive_count()
    );
    eprintln!("shapes: {shapes:?}");
    eprintln!("motion types: {motions:?}");
    eprintln!("layers: {layers:?}");
    eprintln!("bhk block types: {block_types:?}");
    eprintln!("skipped: {skipped:?}");
    if !parse_failures.is_empty() {
        eprintln!(
            "{} files the nif crate could not parse, first: {}",
            parse_failures.len(),
            parse_failures[0]
        );
    }
    assert!(
        bad_bodies.is_empty(),
        "{} bad rigid bodies, first: {}",
        bad_bodies.len(),
        bad_bodies[0]
    );
    assert!(
        huge.is_empty(),
        "{} colliders with absurd extents, first: {}",
        huge.len(),
        huge[0]
    );
    assert!(colliders > 0, "no collider decoded from {files} meshes");
    assert!(
        shapes.contains_key("triangle mesh") && shapes.contains_key("box"),
        "expected both meshes and boxes in a vanilla sample: {shapes:?}"
    );
}

#[test]
fn a_capsule_comes_to_rest_on_the_farmhouse() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let archive =
        soe_bsa::Archive::open(&data.join("Skyrim - Meshes0.bsa")).expect("archive opens");
    let bytes = archive
        .read("meshes/architecture/farmhouse/farmhouse01.nif")
        .expect("the farmhouse is in the archive");
    let file = soe_nif::parse(&bytes).unwrap();
    let c = collect(&file).unwrap();
    let mut world = SimpleWorld::new();
    let mut top = f32::NEG_INFINITY;
    for col in c.colliders.iter().filter(|c| c.is_static()) {
        top = top.max(col.aabb().max.z);
        world.add_static(col);
    }
    assert!(world.static_count() > 0);
    let start = top + 200.0;
    let player = world.create_character(
        soe_physics::CharacterSettings::default(),
        soe_physics::Vec3::new(0.0, 0.0, start),
    );
    for _ in 0..600 {
        world.step(1.0 / 60.0);
    }
    let ch = world.character(player).unwrap();
    eprintln!(
        "farmhouse top at {top}, capsule started at {start}, rests at {:?}",
        ch.position
    );
    assert!(ch.on_ground, "never landed: {ch:?}");
    assert!(
        ch.position.z < start && ch.position.z > -1.0,
        "feet at {}",
        ch.position.z
    );
}
