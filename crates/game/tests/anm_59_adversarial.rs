// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;
const OTHER_ACTOR: u32 = 0x102;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> World {
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
    for object in [ACTOR, OTHER_ACTOR] {
        plugin.push_top(
            Record::new(T(b"ACHR"), object)
                .sub(T(b"NAME"), NPC.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order.add("ANM59.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn anm_59_every_ck_expression_id_keeps_zero_and_full_strength() {
    let mut world = fixture();
    for mood in 0..=16 {
        let strength = if mood % 2 == 0 { 0 } else { 100 };
        world
            .set_expression_override(id(ACTOR), mood, strength)
            .unwrap();
        assert_eq!(
            world.facial_expression(id(ACTOR)),
            Some((mood as u8, strength as u8)),
            "CK expression id {mood} did not retain its requested strength"
        );
    }
}

#[test]
fn anm_59_repeated_clear_uses_latest_actor_mood_and_does_not_clear_another_actor() {
    let mut world = fixture();
    world.set_expression_override(id(ACTOR), 12, 45).unwrap();
    world
        .set_expression_override(id(OTHER_ACTOR), 10, 25)
        .unwrap();
    world.set_actor_value(id(ACTOR), "mood", 6.0).unwrap();
    world.clear_expression_override(id(ACTOR)).unwrap();
    assert_eq!(world.facial_expression(id(ACTOR)), Some((13, 100)));
    assert_eq!(world.facial_expression(id(OTHER_ACTOR)), Some((10, 25)));
    world.clear_expression_override(id(ACTOR)).unwrap();
    assert_eq!(world.facial_expression(id(ACTOR)), Some((13, 100)));
}

#[test]
fn anm_59_failed_mutations_leave_each_actors_expression_unchanged() {
    let mut world = fixture();
    world.set_expression_override(id(ACTOR), 16, 37).unwrap();
    world
        .set_expression_override(id(OTHER_ACTOR), 2, 64)
        .unwrap();
    assert!(world.clear_expression_override(id(NPC)).is_err());
    assert!(world.set_expression_override(id(ACTOR), 16, -1).is_err());
    assert!(world
        .set_expression_override(id(OTHER_ACTOR), 17, 64)
        .is_err());
    assert_eq!(world.facial_expression(id(ACTOR)), Some((16, 37)));
    assert_eq!(world.facial_expression(id(OTHER_ACTOR)), Some((2, 64)));
}
