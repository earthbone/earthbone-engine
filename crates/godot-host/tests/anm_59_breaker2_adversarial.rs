// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};
use soe_godot_host::actor::ActorAssets;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        NPC,
        "Fixture NPC",
        "Fixture NPC",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("ANM59Adversarial.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn anm_59_breaker2_all_ck_moods_accept_boundary_strengths() {
    let mut world = world();
    for mood in 0..=16 {
        for strength in [0, 100] {
            world
                .set_expression_override(id(ACTOR), mood, strength)
                .unwrap();
            assert_eq!(
                world.facial_expression(id(ACTOR)),
                Some((mood as u8, strength as u8))
            );
        }
    }
}

#[test]
fn anm_59_breaker2_invalid_override_preserves_saved_override() {
    let mut world = world();
    world.set_expression_override(id(ACTOR), 16, 37).unwrap();
    let before = world.save_state();
    assert!(world.set_expression_override(id(ACTOR), -1, 50).is_err());
    assert!(world.set_expression_override(id(ACTOR), 0, -1).is_err());
    assert_eq!(world.facial_expression(id(ACTOR)), Some((16, 37)));
    assert_eq!(world.save_state(), before);
}

#[test]
fn anm_59_breaker2_adversarial_installed_head_shapes_are_supported_and_aligned() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed head expression survey");
        return;
    };
    let mut vfs = Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = data.join(archive);
        if !path.exists() {
            eprintln!(
                "{} is absent, skipping installed head expression survey",
                path.display()
            );
            return;
        }
        vfs.add_archive_path(Path::new(&path)).unwrap();
    }
    let assets = ActorAssets::load(
        &vfs,
        "meshes/actors/character/character assets/femalehead.nif",
    )
    .unwrap();
    let shapes = assets
        .surfaces
        .iter()
        .flat_map(|surface| {
            surface
                .expression_shapes
                .iter()
                .map(move |shape| (surface, shape))
        })
        .collect::<Vec<_>>();
    assert!(
        !shapes.is_empty(),
        "installed head has no expression targets"
    );
    for (surface, shape) in shapes {
        assert!(
            soe_godot_host::facial::expression_name_for_target(&shape.name).is_some(),
            "unsupported expression target {} was exposed",
            shape.name
        );
        assert_eq!(shape.positions.len(), surface.arrays.positions.len());
        assert!(shape
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
    }
}
