// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};
use soe_godot_host::facial::{bound_actor_expression_weights, expression_name, expression_weights};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;
const OTHER_ACTOR: u32 = 0x102;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
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
    for actor in [ACTOR, OTHER_ACTOR] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), NPC.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order.add("ANM59BoundHeadAdversarial.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn anm_59_breaker3_every_ck_expression_target_gets_its_selected_weight() {
    let names = (0..=16)
        .map(|mood| expression_name(mood).unwrap().to_owned())
        .collect::<Vec<_>>();
    let weights = expression_weights(&names, 12, 37);
    assert_eq!(weights.len(), 17);
    assert_eq!(weights["MoodSurprise"], 0.37);
    assert!(names
        .iter()
        .filter(|name| name.as_str() != "MoodSurprise")
        .all(|name| weights[name] == 0.0));
}

#[test]
fn anm_59_breaker3_repeated_override_switch_and_clear_are_actor_local() {
    let mut world = world();
    let targets = [
        "MoodHappy".to_owned(),
        "MoodSurprise".to_owned(),
        "CombatShout".to_owned(),
        "DialogueHappy".to_owned(),
    ];

    world.set_expression_override(id(ACTOR), 12, 45).unwrap();
    world
        .set_expression_override(id(OTHER_ACTOR), 2, 100)
        .unwrap();
    world.set_expression_override(id(ACTOR), 16, 0).unwrap();
    let switched = bound_actor_expression_weights(&mut world, id(ACTOR), &targets);
    assert_eq!(switched["CombatShout"], 0.0);
    assert_eq!(switched["MoodSurprise"], 0.0);

    world.clear_expression_override(id(ACTOR)).unwrap();
    let resumed = bound_actor_expression_weights(&mut world, id(ACTOR), &targets);
    let other = bound_actor_expression_weights(&mut world, id(OTHER_ACTOR), &targets);
    assert_eq!(resumed["MoodHappy"], 1.0);
    assert_eq!(other["MoodHappy"], 0.0);
    assert_eq!(other["DialogueHappy"], 1.0);
}

#[test]
fn anm_59_breaker3_missing_actor_has_no_bound_expression_targets() {
    let mut world = world();
    let targets = ["MoodHappy".to_owned(), "MoodSurprise".to_owned()];
    assert!(bound_actor_expression_weights(&mut world, id(0xDEAD), &targets).is_empty());
}

#[test]
fn anm_59_breaker3_bound_expression_survives_engine_world_save_load() {
    let mut source = world();
    let targets = ["MoodHappy".to_owned(), "MoodSurprise".to_owned()];
    source.set_expression_override(id(ACTOR), 12, 45).unwrap();
    let saved = source.save_state();

    let mut restored = world();
    restored.load_state(&saved).unwrap();
    let weights = bound_actor_expression_weights(&mut restored, id(ACTOR), &targets);
    assert_eq!(weights["MoodSurprise"], 0.45);
    assert_eq!(weights["MoodHappy"], 0.0);
}
