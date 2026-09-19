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
