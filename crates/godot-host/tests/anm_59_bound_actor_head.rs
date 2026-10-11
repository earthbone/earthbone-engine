// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};
use soe_godot_host::actor::ActorAssets;
use soe_godot_host::facial::bound_actor_expression_weights;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;

fn actor_id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: ACTOR,
    }
}

fn bound_actor_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut ai_data = vec![0; 20];
    ai_data[4] = 3;
    plugin.push_top(
        npc::build_record(
            NPC,
            "Fixture NPC",
            "Fixture NPC",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"AIDT"), ai_data),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("ANM59BoundHead.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn anm_59_bound_actor_head_applies_override_strength_and_resumes_mood_after_clear() {
    let mut world = bound_actor_world();
    let targets = ["MoodHappy".to_owned(), "MoodSurprise".to_owned()];

    world.set_expression_override(actor_id(), 12, 45).unwrap();
    let override_weights = bound_actor_expression_weights(&mut world, actor_id(), &targets);
    assert_eq!(override_weights["MoodSurprise"], 0.45);
    assert_eq!(override_weights["MoodHappy"], 0.0);

    world.clear_expression_override(actor_id()).unwrap();
    let resumed_weights = bound_actor_expression_weights(&mut world, actor_id(), &targets);
    assert_eq!(resumed_weights["MoodSurprise"], 0.0);
    assert_eq!(resumed_weights["MoodHappy"], 1.0);
}

#[test]
fn anm_59_vanilla_head_targets_receive_bound_actor_override_and_mood_weights() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed bound head check");
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
                "{} is absent, skipping installed bound head check",
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
    assert!(targets.iter().any(|name| name == "MoodSurprise"));
    assert!(targets.iter().any(|name| name == "MoodHappy"));

    let mut world = bound_actor_world();
    world.set_expression_override(actor_id(), 12, 45).unwrap();
    let override_weights = bound_actor_expression_weights(&mut world, actor_id(), &targets);
    assert_eq!(override_weights["MoodSurprise"], 0.45);
    assert_eq!(override_weights["MoodHappy"], 0.0);

    world.clear_expression_override(actor_id()).unwrap();
    let resumed_weights = bound_actor_expression_weights(&mut world, actor_id(), &targets);
    assert_eq!(resumed_weights["MoodSurprise"], 0.0);
    assert_eq!(resumed_weights["MoodHappy"], 1.0);
}
