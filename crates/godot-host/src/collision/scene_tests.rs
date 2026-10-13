// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed-scene regressions for candle supports and sudden floor drops.

use super::*;
use soe_esm::Tag;
use soe_physics::{CharacterSettings, Vec3};

#[test]
fn washbasin_feet_reach_the_authored_origin() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).expect("data opens");
    let bytes = source
        .vfs()
        .read("meshes/furniture/noble/noblewashbasin01.nif")
        .expect("basin");
    let placed = crate::data_source::placed_model(&bytes).expect("placed basin");
    let mut lowest = f32::INFINITY;
    for mesh in placed.meshes().expect("basin surfaces") {
        for p in mesh.positions {
            lowest = lowest.min(mesh.transform.apply(soe_nif::Vec3::new(p[0], p[1], p[2])).z);
        }
    }
    assert!(
        lowest.abs() < 1.0,
        "washbasin floats {lowest} units above its placement origin"
    );
}

#[test]
fn bannered_mare_container_supports_are_drawn_and_solid() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping container support integration");
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).expect("data opens");
    let cell = source.resolve_cell("WhiterunBanneredMare").expect("cell");
    let loaded = source.stream_cell_id(cell).expect("stream cell");
    let drawn: std::collections::BTreeSet<_> = loaded
        .surfaces
        .iter()
        .map(|surface| surface.reference)
        .collect();
    let mut containers = 0;
    let mut supports = 0;
    for &id in source.cell_index().references(cell) {
        let rref = source.order().winner_ref(id).expect("reference");
        let record = source.order().record(rref);
        if record.tag != tag::REFR {
            continue;
        }
        let refr = Refr::from_record(record).expect("decode reference");
        let base = source
            .order()
            .winner(source.order().resolve(rref.file, refr.base))
            .expect("base");
        if base.tag != Tag::new(b"CONT") {
            continue;
        }
        let Some(model) = base.find(Tag::new(b"MODL")).map(soe_esm::cursor::zstring) else {
            continue;
        };
        if model.is_empty() {
            continue;
        }
        containers += 1;
        assert!(drawn.contains(&id), "container {id} {model} is invisible");
        let name = model.to_ascii_lowercase();
        if ["nighttable", "chestdrawers", "wardrobe", "endtable"]
            .iter()
            .any(|part| name.contains(part))
        {
            supports += 1;
            let collision = source
                .colliders_of(&model, &refr)
                .expect("support collision");
            assert!(
                collision.colliders.iter().any(Collider::is_static),
                "support {id} {model} has no static collision"
            );
        }
    }
    assert!(supports >= 4, "fixture lost its candle-support furniture");
    assert_eq!(loaded.stats.containers_with_model, containers);
    eprintln!("container references {containers}, checked supports {supports}");
}

#[test]
fn bannered_mare_walking_routes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping walking routes");
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).expect("data opens");
    let cell = source.resolve_cell("WhiterunBanneredMare").expect("cell");
    // Upstairs and kitchen formerly jumped 28.8 and 21.7 units in one tick.
    // The main route crosses furniture edges, where a full step is allowed.
    for (name, spawn, direction, limit) in [
        (
            "upstairs",
            Vec3::new(0.0, 0.0, 500.0),
            Vec3::new(0.0, 1.0, 0.0),
            8.0,
        ),
        (
            "main",
            Vec3::new(-128.0, -850.0, 150.0),
            Vec3::new(0.0, 1.0, 0.0),
            36.0,
        ),
        (
            "kitchen",
            Vec3::new(-1152.0, -384.0, 150.0),
            Vec3::new(-1.0, 0.0, 0.0),
            8.0,
        ),
    ] {
        let mut world = source.collide_cell_id(cell).expect("collision").world;
        let id = world.create_character(CharacterSettings::default(), spawn);
        for _ in 0..240 {
            world.step(1.0 / 60.0);
        }
        let mut previous = world.character(id).expect("character").position;
        let mut largest = 0.0_f32;
        for tick in 0..300 {
            world.walk(id, direction, soe_physics::JOG_SPEED);
            world.step(1.0 / 60.0);
            let next = world.character(id).expect("character").position;
            let dz = (next.z - previous.z).abs();
            largest = largest.max(dz);
            assert!(next.x.is_finite() && next.y.is_finite() && next.z.is_finite());
            assert!(
                dz <= limit,
                "{name} tick {tick} jumps {dz} units: {previous:?} -> {next:?}"
            );
            previous = next;
        }
        eprintln!("WALK {name} largest vertical change {largest}");
    }
}

#[test]
fn tst_19_container_corridor_reaches_every_waypoint_in_five_physics_seconds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).expect("data opens");
    let cell = source.resolve_cell("WhiterunBanneredMare").expect("cell");
    let game = soe_game::Game::from_shared_order(source.shared_order());
    let anchor = game
        .world_mut()
        .reference(soe_esm::GlobalFormId {
            file: 0,
            object: 0x060359,
        })
        .expect("barrel")
        .position;
    let collision = source.collide_cell_id(cell).expect("collision");
    let mut world = collision.world;
    let id = world.create_character(
        CharacterSettings::default(),
        Vec3::new(166.1684, -576.653, 69.2457),
    );
    for _ in 0..60 {
        world.step_character(id, 1.0 / 60.0);
    }
    let feet = world.character(id).expect("character").position;
    let path = soe_game::play::navigation_approach_path_with_portal_inset(
        &game,
        cell,
        [feet.x, feet.y, feet.z],
        anchor,
        128.0,
        CharacterSettings::default().radius,
    )
    .expect("container corridor");
    let mut waypoint = 0;
    for tick in 0..300 {
        let feet = world.character(id).expect("character").position;
        while let Some(point) = path.points.get(waypoint) {
            let arrival = if waypoint == path.points.len() - 1 {
                36.0
            } else {
                16.0
            };
            if (point[0] - feet.x).hypot(point[1] - feet.y) >= arrival {
                break;
            }
            waypoint += 1;
        }
        if waypoint == path.points.len() {
            eprintln!(
                "TST19_CONTAINER physics_s={} waypoints={waypoint} feet={feet:?}",
                tick as f32 / 60.0
            );
            return;
        }
        let point = path.points[waypoint];
        world.walk(
            id,
            Vec3::new(point[0] - feet.x, point[1] - feet.y, 0.0),
            soe_physics::JOG_SPEED,
        );
        world.step_character(id, 1.0 / 60.0);
    }
    let feet = world.character(id).expect("character").position;
    for angle in 0..8 {
        let angle = angle as f32 * std::f32::consts::FRAC_PI_4;
        let from = feet + Vec3::Z * 64.0;
        let to = from + Vec3::new(angle.cos(), angle.sin(), 0.0) * 96.0;
        if let Some(hit) = world.raycast(from, to) {
            let reference = collision.references.get(&hit.body).copied();
            let model = reference.and_then(|reference| {
                let owner = source.order().winner_ref(reference)?;
                let refr = Refr::from_record(source.order().record(owner)).ok()?;
                model_for(
                    source
                        .order()
                        .winner(source.order().resolve(owner.file, refr.base))?,
                )
            });
            eprintln!(
                "CONTAINER_CONTACT reference={reference:?} model={model:?} point={:?} normal={:?}",
                hit.point, hit.normal
            );
        }
    }
    panic!(
        "container waypoint {waypoint}/{} feet={feet:?} points={:?}",
        path.points.len(),
        path.points
    );
}

#[test]
fn tst_19_loose_corridor_stays_above_the_inn_floor() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).expect("data opens");
    let cell = source.resolve_cell("WhiterunBanneredMare").expect("cell");
    let game = soe_game::Game::from_shared_order(source.shared_order());
    let anchor = game
        .world_mut()
        .reference(soe_esm::GlobalFormId {
            file: 0,
            object: 0x059050,
        })
        .expect("loose item")
        .position;
    for catchup in [1, 2] {
        for offset in [-1.0, 0.0, 1.0] {
            let collision = source.collide_cell_id(cell).expect("collision");
            let mut world = collision.world;
            let id = world.create_character(
                CharacterSettings::default(),
                Vec3::new(457.1329 + offset, -518.4153, 67.9895),
            );
            for _ in 0..60 {
                world.step_character(id, 1.0 / 60.0);
            }
            let feet = world.character(id).expect("character").position;
            let path = soe_game::play::navigation_approach_path_with_portal_inset(
                &game,
                cell,
                [feet.x, feet.y, feet.z],
                anchor,
                128.0,
                CharacterSettings::default().radius,
            )
            .expect("loose corridor");
            let mut waypoint = 0;
            let mut tick = 0;
            let mut walking_seconds = 0.0;
            // Match the walker's logical clock and arrival-before-budget order.
            loop {
                let feet = world.character(id).expect("character").position;
                assert!(feet.z > 36.0, "catchup={catchup} offset={offset} tick={tick} waypoint={waypoint} feet={feet:?}");
                while let Some(point) = path.points.get(waypoint) {
                    let arrival = if waypoint == path.points.len() - 1 {
                        36.0
                    } else {
                        16.0
                    };
                    if (point[0] - feet.x).hypot(point[1] - feet.y) >= arrival {
                        break;
                    }
                    waypoint += 1;
                }
                if waypoint == path.points.len() {
                    break;
                }
                if walking_seconds >= 5.0 {
                    break;
                }
                let point = path.points[waypoint];
                let yaw = f64::from(point[0] - feet.x).atan2(f64::from(point[1] - feet.y)) as f32;
                let mut camera = crate::camera::CameraRig::default();
                camera.set_facing(yaw, 0.0);
                world.walk(id, camera.move_direction(0.0, 1.0), soe_physics::JOG_SPEED);
                for _ in 0..catchup {
                    world.step_character(id, 1.0 / 60.0);
                    walking_seconds += 1.0 / 60.0;
                    tick += 1;
                }
            }
            let feet = world.character(id).expect("character").position;
            while let Some(point) = path.points.get(waypoint) {
                let arrival = if waypoint == path.points.len() - 1 {
                    36.0
                } else {
                    16.0
                };
                if (point[0] - feet.x).hypot(point[1] - feet.y) >= arrival {
                    break;
                }
                waypoint += 1;
            }
            assert!(feet.z > 36.0, "final feet={feet:?}");
            assert_eq!(
                waypoint,
                path.points.len(),
                "catchup={catchup} offset={offset} feet={feet:?} goal={:?}",
                path.points.last()
            );
        }
    }
}
