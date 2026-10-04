// SPDX-License-Identifier: GPL-3.0-or-later

//! A shipped-cell regression for furniture omitted by the STAT-only path.

use super::*;
use soe_esm::Tag;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn placed_floor_discards_export_root_but_preserves_children() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping floor placement integration");
        return;
    };
    let source = DataSource::open(&data).expect("open data");
    let bytes = source
        .vfs()
        .read("meshes/architecture/whiterun/wrinteriors/wrintfloordirtstr01.nif")
        .expect("floor asset");
    let raw = soe_nif::parse(&bytes).expect("parse floor");
    let placed = crate::data_source::placed_model(&bytes).expect("place floor");
    let soe_nif::blocks::Block::Node(root) = &raw.blocks[0] else {
        panic!("expected node root");
    };
    assert!(
        root.av.transform.rotation.rows[0][1].abs() > 0.9,
        "fixture no longer has its quarter-turn export root"
    );
    let soe_nif::blocks::Block::Node(root) = &placed.blocks[0] else {
        panic!("expected placed node root");
    };
    assert_eq!(root.av.transform, soe_nif::Transform::IDENTITY);
    assert_eq!(
        raw.blocks[1..],
        placed.blocks[1..],
        "placement altered child or collision blocks"
    );
    for mesh in placed.meshes().expect("floor surfaces") {
        assert_eq!(mesh.transform, soe_nif::Transform::IDENTITY);
    }
}

#[test]
fn bannered_mare_furniture_is_drawn_and_beds_have_collision() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping furniture integration");
        return;
    };
    let source = DataSource::open(&data).expect("open installed data");
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("Bannered Mare exists");
    let loaded = source.stream_cell_id(cell).expect("stream Bannered Mare");
    let collision = source.collide_cell_id(cell).expect("cell collision builds");
    let collision_stats = &collision.stats;
    assert_eq!(
        collision_stats.references,
        loaded.stats.statics_with_model
            + loaded.stats.furniture_with_model
            + loaded.stats.containers_with_model
            + loaded.stats.activators_with_model
            + loaded.stats.doors_with_model,
        "rendering and collision select different model-bearing references"
    );
    let drawn: BTreeSet<_> = loaded.surfaces.iter().map(|s| s.reference).collect();
    let mut furniture = 0;
    let mut beds = 0;
    let mut doors = 0;
    let mut omitted = Vec::new();
    for &id in source.cell_index().references(cell) {
        let rref = source.order().winner_ref(id).expect("reference resolves");
        let rec = source.order().record(rref);
        if rec.tag != tag::REFR {
            continue;
        }
        let refr = Refr::from_record(rec).expect("reference decodes");
        let base = source
            .order()
            .winner(source.order().resolve(rref.file, refr.base))
            .expect("base resolves");
        if base.tag == Tag::new(b"DOOR") && base.flags & (1 << 23) == 0 {
            let Some(model) = model_for(base) else {
                continue;
            };
            doors += 1;
            assert!(drawn.contains(&id), "door {id} has no visible surfaces");
            assert!(
                collision
                    .references
                    .values()
                    .any(|reference| *reference == id),
                "door {id} has no mapped collision body"
            );
            let collected = source.colliders_of(&model, &refr).expect("door collision");
            let reachable = collected
                .colliders
                .iter()
                .filter(|c| c.is_static())
                .any(|c| {
                    let bounds = c.aabb();
                    let center = (bounds.min + bounds.max) * 0.5;
                    let half = (bounds.max - bounds.min) * 0.5;
                    [
                        soe_physics::Vec3::new(half.x + 16.0, 0.0, 0.0),
                        soe_physics::Vec3::new(0.0, half.y + 16.0, 0.0),
                        soe_physics::Vec3::new(0.0, 0.0, half.z + 16.0),
                    ]
                    .into_iter()
                    .any(|offset| {
                        [1.0, -1.0].into_iter().any(|sign| {
                            collision
                                .world
                                .raycast(center + offset * sign, center - offset * sign)
                                .is_some_and(|hit| collision.references.get(&hit.body) == Some(&id))
                        })
                    })
                });
            assert!(
                reachable,
                "door {id} cannot be picked through its collision bounds"
            );
            eprintln!("door {id}: {model}, mapped collision and ray hit");
        }
        if base.tag != Tag::new(b"FURN") || base.flags & (1 << 23) != 0 {
            continue;
        }
        let Some(model) = base.find(Tag::new(b"MODL")).map(soe_esm::cursor::zstring) else {
            continue;
        };
        if model.is_empty() {
            continue;
        }
        furniture += 1;
        if !drawn.contains(&id) {
            omitted.push((id, model.clone()));
        }
        if model.to_ascii_lowercase().contains("bed") {
            beds += 1;
            eprintln!("bed {id}: {model}, placement {:?}", refr.transform);
            let collected = source
                .colliders_of(&model, &refr)
                .expect("bed collision reads");
            assert!(
                collected.colliders.iter().any(Collider::is_static),
                "bed {id} has no supported static collision"
            );
        }
    }
    eprintln!(
        "furniture {furniture}, beds {beds}, omitted {}",
        omitted.len()
    );
    assert!(furniture > 0, "fixture contains no furniture");
    assert!(beds > 0, "fixture contains no beds");
    assert!(doors > 0, "fixture contains no doors");
    assert_eq!(loaded.stats.doors_with_model, doors);
    assert_eq!(collision.references.len(), collision_stats.bodies);
    assert_eq!(loaded.stats.furniture_with_model, furniture);
    assert!(
        omitted.is_empty(),
        "furniture absent from render output: {omitted:?}"
    );
}
