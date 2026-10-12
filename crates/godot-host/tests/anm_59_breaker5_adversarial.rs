// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};
use soe_godot_host::{actor::ActorAssets, facial::bound_actor_expression_weights};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;

fn female_actor_world() -> (World, GlobalFormId) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        NPC,
        "Female fixture",
        "Female fixture",
        npc::Config {
            flags: npc::flags::FEMALE,
            ..npc::Config::default()
        },
        npc::Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("ANM59Female.esm", plugin).unwrap();
    (
        World::new(order),
        GlobalFormId {
            file: 0,
            object: ACTOR,
        },
    )
}

#[test]
fn anm_59_breaker5_ck_female_combat_expression_does_not_reach_head_weights() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed female head check");
        return;
    };
    let mut vfs = Vfs::new();
    for archive in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let path = data.join(archive);
        if !path.exists() {
            eprintln!(
                "{} is absent, skipping installed female head check",
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
    let targets = assets
        .surfaces
        .iter()
        .flat_map(|surface| &surface.expression_shapes)
        .map(|shape| shape.name.clone())
        .collect::<Vec<_>>();
    let (mut world, actor) = female_actor_world();

    world.set_expression_override(actor, 15, 100).unwrap();
    let weights = bound_actor_expression_weights(&mut world, actor, &targets);

    assert!(
        !weights.contains_key("CombatAnger") || weights["CombatAnger"] == 0.0,
        "CK says female combat expressions do not work, got {:?}",
        weights.get("CombatAnger")
    );
}
