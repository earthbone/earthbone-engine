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

/// Builds the Bannered Mare's collision out of the shipped plugins and
/// meshes, then walks a capsule around inside it.
///
/// The synthetic tests prove the controller against angles that are exact.
/// This one proves it against the geometry a player actually stands on,
/// which is where the awkward cases live: thresholds, stair treads, the
/// hearth, and the gaps between placed pieces.
#[test]
fn a_capsule_walks_around_the_bannered_mare() {
    use soe_esm::records::{refr::Refr, stat::Static};
    use soe_esm::{tag, LoadOrder, Plugin};
    use soe_physics::{CharacterSettings, Iso, Vec3};

    const MASTERS: [&str; 5] = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];

    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the cell walk");
        return;
    };

    let mut order = LoadOrder::new();
    for name in MASTERS {
        let bytes = std::fs::read(data.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order
            .add(name, plugin)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let index = order.build_cell_index();
    let cell = index
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("the inn is in the base game");
    let placed_objects: Vec<_> = index.references(cell).to_vec();

    let archives: Vec<_> = ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"]
        .iter()
        .filter_map(|n| soe_bsa::Archive::open(&data.join(n)).ok())
        .collect();
    assert!(!archives.is_empty(), "no mesh archive opened");
    let read_mesh =
        |path: &str| -> Option<Vec<u8>> { archives.iter().find_map(|a| a.read(path).ok()) };

    let started = Instant::now();
    let mut world = SimpleWorld::new();
    let (mut placed, mut without_model, mut failed) = (0usize, 0usize, 0usize);
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for id in placed_objects {
        let Some(rref) = order.winner_ref(id) else {
            continue;
        };
        let rec = order.record(rref);
        if rec.tag != tag::REFR {
            continue;
        }
        let Ok(refr) = Refr::from_record(rec) else {
            continue;
        };
        let base = order.resolve(rref.file, refr.base);
        let Some(base_ref) = order.winner_ref(base) else {
            continue;
        };
        let base_record = order.record(base_ref);
        if base_record.tag != tag::STAT {
            continue;
        }
        let Ok(stat) = Static::from_record(base_record) else {
            continue;
        };
        let Some(model) = stat.model.as_ref() else {
            without_model += 1;
            continue;
        };
        let path = format!("meshes/{}", model.replace('\\', "/").to_lowercase());
        let Some(bytes) = read_mesh(&path) else {
            failed += 1;
            continue;
        };
        let Ok(file) = soe_nif::parse(&bytes) else {
            failed += 1;
            continue;
        };
        let Ok(collected) = collect(&file) else {
            failed += 1;
            continue;
        };
        let t = refr.transform.unwrap_or_default();
        let place = Iso::placement(t.position, t.rotation, refr.scale);
        let mut any = false;
        for col in collected.colliders.iter().filter(|c| c.is_static()) {
            let mut moved = col.clone();
            moved.transform = place.then(&col.transform);
            let aabb = moved.aabb();
            bounds = Some(match bounds {
                None => (aabb.min, aabb.max),
                Some((lo, hi)) => (lo.min(aabb.min), hi.max(aabb.max)),
            });
            world.add_static(&moved);
            any = true;
        }
        if any {
            placed += 1;
        }
    }
    let build = started.elapsed();
    let (lo, hi) = bounds.expect("the inn has collision");
    eprintln!(
        "bannered mare: {placed} references with collision, {without_model} without a model, \
         {failed} unreadable, {} bodies, {} primitives, built in {:.3}s",
        world.static_count(),
        world.primitive_count(),
        build.as_secs_f32()
    );
    eprintln!("collision bounds {lo:?} to {hi:?}");
    assert!(placed > 0, "no collision was placed");

    // Find the ground floor rather than guessing a height. Rays from above
    // the whole cell are no use, they hit the roof first, so these start head
    // height above the lowest collision, which is inside the ground floor
    // room. The most popular hit height is then the floor itself, where a
    // table or a stair tread covers only a few samples.
    let mut floors: BTreeMap<i32, Vec<Vec3>> = BTreeMap::new();
    let steps = 16;
    for ix in 0..steps {
        for iy in 0..steps {
            let fx = (ix as f32 + 0.5) / steps as f32;
            let fy = (iy as f32 + 0.5) / steps as f32;
            let x = lo.x + (hi.x - lo.x) * fx;
            let y = lo.y + (hi.y - lo.y) * fy;
            let from = Vec3::new(x, y, lo.z + 150.0);
            let to = Vec3::new(x, y, lo.z - 10.0);
            if let Some(hit) = world.raycast(from, to) {
                if hit.normal.z > 0.7 {
                    let bucket = (hit.point.z / 8.0).round() as i32;
                    floors.entry(bucket).or_default().push(hit.point);
                }
            }
        }
    }
    for (b, pts) in &floors {
        eprintln!(
            "  floor candidate z {:.0}: {} samples",
            *b as f32 * 8.0,
            pts.len()
        );
    }
    let (bucket, points) = floors
        .iter()
        .max_by_key(|(_, v)| v.len())
        .expect("rays found a floor somewhere in the inn");
    let stand_on = points[points.len() / 2];
    eprintln!(
        "floor height {:.1} carries {} of {} sampled points, starting at {stand_on:?}",
        *bucket as f32 * 8.0,
        points.len(),
        steps * steps
    );

    let settings = CharacterSettings::default();
    let id = world.create_character(settings, stand_on + Vec3::Z * 12.0);
    let mut ground_frames = 0usize;
    let mut total_frames = 0usize;
    let mut lowest = f32::MAX;
    let step = |world: &mut SimpleWorld,
                frames: usize,
                ground_frames: &mut usize,
                total: &mut usize,
                lowest: &mut f32| {
        for _ in 0..frames {
            world.step(1.0 / 60.0);
            let ch = world.character(id).unwrap();
            *total += 1;
            if ch.on_ground {
                *ground_frames += 1;
            }
            *lowest = lowest.min(ch.position.z);
        }
        world.character(id).unwrap().position
    };

    let settled = step(
        &mut world,
        180,
        &mut ground_frames,
        &mut total_frames,
        &mut lowest,
    );
    eprintln!("settled at {settled:?}");
    assert!(
        world.character(id).unwrap().on_ground,
        "never found a floor inside the inn, ended at {settled:?}"
    );

    // Four legs of the compass, each a second of walking.
    let legs = [
        ("+x", Vec3::new(1.0, 0.0, 0.0)),
        ("+y", Vec3::new(0.0, 1.0, 0.0)),
        ("-x", Vec3::new(-1.0, 0.0, 0.0)),
        ("-y", Vec3::new(0.0, -1.0, 0.0)),
    ];
    let mut moved_any = false;
    let mut wedged = Vec::new();
    for (name, dir) in legs {
        let before = world.character(id).unwrap().position;
        world.walk(id, dir, soe_physics::WALK_SPEED);
        let after = step(
            &mut world,
            60,
            &mut ground_frames,
            &mut total_frames,
            &mut lowest,
        );
        let travelled = ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt();
        eprintln!(
            "leg {name}: {travelled:.1} units, {before:?} to {after:?}, ground={}",
            world.character(id).unwrap().on_ground
        );
        if travelled > 50.0 {
            moved_any = true;
        }
        if travelled < 5.0 {
            wedged.push(name);
        }
    }
    world.walk(id, Vec3::ZERO, 0.0);

    let on_ground_share = ground_frames as f32 / total_frames as f32;
    eprintln!(
        "on ground for {ground_frames} of {total_frames} frames ({:.0}%), lowest z {lowest:.1}, \
         legs that did not move: {wedged:?}",
        on_ground_share * 100.0
    );

    assert!(
        moved_any,
        "the capsule could not walk anywhere inside the inn"
    );
    assert!(
        lowest > lo.z - 100.0,
        "fell out of the world, lowest z {lowest} against a floor near {}",
        lo.z
    );
    assert!(
        on_ground_share > 0.8,
        "spent {:.0}% of the walk airborne, which means it is bouncing or falling",
        (1.0 - on_ground_share) * 100.0
    );
}
