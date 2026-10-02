// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const NPC: u32 = 0x100;
const ACTOR: u32 = 0x101;

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
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("ANM59.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn anm_59_breaker_override_switch_and_clear_survive_engine_save_load() {
    let mut world = fixture();
    world.set_expression_override(id(ACTOR), 12, 0).unwrap();
    world.set_expression_override(id(ACTOR), 16, 100).unwrap();
    let save = world.save_state();

    let mut restored = fixture();
    restored.load_state(&save).unwrap();
    assert_eq!(restored.facial_expression(id(ACTOR)), Some((16, 100)));
    restored.clear_expression_override(id(ACTOR)).unwrap();
    assert_eq!(restored.facial_expression(id(ACTOR)), Some((10, 100)));
}
