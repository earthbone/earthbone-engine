// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-16 trainer cap boundary checks.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn dlg_16_training_never_crosses_class_skill_cap() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x800).sub(T(b"EDID"), b"Gold001\0".to_vec()));
    let mut class_data = vec![0; 36];
    class_data[4] = 0;
    class_data[5] = 1;
    plugin.push_top(Record::new(T(b"CLAS"), 0x801).sub(T(b"DATA"), class_data));
    let trainer = npc::build_record(
        0x802,
        "Training Actor",
        "Training Actor",
        npc::Config::default(),
        npc::Stats::default(),
    )
    .sub(T(b"CNAM"), 0x801u32.to_le_bytes());
    plugin.push_top(trainer);
    plugin.push_top(
        Record::new(T(b"GMST"), 0x803)
            .sub(T(b"EDID"), b"fTrainingMultCost\0".to_vec())
            .sub(T(b"DATA"), 10.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Training.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x802), player, false);
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 24.5).unwrap();
        world.add_item(player, id(0x800), 500);
    }
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    let result = game
        .world_mut()
        .purchase_training(trainer, &mut allocate);

    assert!(
        result.is_err() || game.world_mut().base_actor_value(player, "onehanded") <= 25.0,
        "CLAS maximum training level is a cap, so a fractional skill cannot train above it"
    );
}
