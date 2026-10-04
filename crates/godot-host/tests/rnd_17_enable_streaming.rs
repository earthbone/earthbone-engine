// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 installed Whiterun battle references use the shared enable-parent query.

use soe_esm::{plugin::flags, tag};
use soe_game::World;
use soe_godot_host::data_source::DataSource;

#[test]
fn rnd_17_whiterun_barricade_has_no_surface_or_collision_until_parent_enabled() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
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
        if order
            .record(cell_owner)
            .placement
            .world
            .map(|raw| order.resolve(cell_owner.file, raw))
            != Some(whiterun)
        {
            continue;
        }
        if order
            .record(cell_owner)
            .find(soe_esm::Tag::new(b"XCLC"))
            .is_none()
            || order.record(cell_owner).editor_id().is_none()
        {
            continue;
        }
        let Some(base_raw) = record
            .find(soe_esm::Tag::new(b"NAME"))
            .and_then(|bytes| bytes.get(..4))
        else {
            continue;
        };
        let base = order.resolve(owner.file, u32::from_le_bytes(base_raw.try_into().unwrap()));
        if order
            .winner(base)
            .and_then(|record| record.find(soe_esm::Tag::new(b"MODL")))
            .is_none()
        {
            continue;
        }
        if !order
            .winner(base)
            .and_then(|record| record.editor_id())
            .is_some_and(|name| name.to_ascii_lowercase().contains("barricade"))
        {
            continue;
        }
        let Some(parent) = record
            .find(soe_esm::Tag::new(b"XESP"))
            .filter(|data| data.len() >= 8)
        else {
            continue;
        };
        if parent[4] & 1 != 0 {
            continue;
        }
        let parent = order.resolve(
            owner.file,
            u32::from_le_bytes(parent[..4].try_into().unwrap()),
        );
        if order.winner(base).unwrap().tag == soe_esm::Tag::new(b"ACTI")
            && order.winner(parent).is_some_and(|record| {
                record.flags & flags::INITIALLY_DISABLED != 0
                    && record.find(soe_esm::Tag::new(b"XESP")).is_none()
            })
        {
            candidates.push((id, cell, parent));
        }
    }
    candidates.sort();
    eprintln!("RND-17 candidates: {candidates:?}");
    let &(barricade, cell, marker) = candidates
        .first()
        .expect("Whiterun battle barricade with disabled marker");
    let placement = order
        .winner(barricade)
        .unwrap()
        .find(soe_esm::Tag::new(b"DATA"))
        .unwrap();
    let coordinate = order
        .winner(cell)
        .unwrap()
        .find(soe_esm::Tag::new(b"XCLC"))
        .unwrap();
    let field = |data: &[u8], offset: usize| {
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    };
    let manifest = format!(
        "{{\"reference\":{},\"marker\":{},\"file\":{},\"cell\":\"{}\",\"x\":{},\"y\":{},\"px\":{},\"py\":{},\"pz\":{}}}",
        barricade.object, marker.object, barricade.file,
        order.winner(cell).unwrap().editor_id().unwrap(),
        field(coordinate, 0) as i32, field(coordinate, 4) as i32,
        f32::from_bits(field(placement, 0)), f32::from_bits(field(placement, 4)), f32::from_bits(field(placement, 8)),
    );
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
        });
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("rnd_17_whiterun.json"), manifest).unwrap();
    let mut world = World::from_shared_order(order);
    assert!(!world.reference_enabled(barricade));
    let loaded = source.stream_cell_id(cell).unwrap();
    assert!(!loaded
        .surfaces
        .iter()
        .any(|surface| surface.reference == barricade));
    let collision = source.collide_cell_id(cell).unwrap();
    assert!(!collision.references.values().any(|id| *id == barricade));
    world.set_reference_enabled(marker, true);
    assert!(world.reference_enabled(barricade));
    assert!(!source
        .stream_reference(barricade)
        .unwrap()
        .surfaces
        .is_empty());
    assert!(source
        .placed_reference_colliders(barricade)
        .unwrap()
        .iter()
        .any(|placed| placed.collider.is_static()));
    eprintln!("RND-17 barricade={barricade} cell={cell} marker={marker}");
    eprintln!(
        "RND-17 cell editor={:?} placement={:?}",
        source.shared_order().winner(cell).unwrap().editor_id(),
        world.ref_placement(barricade)
    );
}
