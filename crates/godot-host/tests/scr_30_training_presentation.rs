// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_godot_host::training_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn training_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x800).sub(T(b"EDID"), b"Gold001\0".to_vec()));
    let mut class_data = vec![0; 36];
    class_data[4] = 0;
    class_data[5] = 1;
    plugin.push_top(Record::new(T(b"CLAS"), 0x801).sub(T(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x802,
            "Trainer",
            "Trainer",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"CNAM"), 0x801u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x803)
            .sub(T(b"EDID"), b"fTrainingMultCost\0".to_vec())
            .sub(T(b"DATA"), 10.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr30Training.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_30_training_presentation_uses_offer_and_unavailable_states() {
    let mut game = training_game();
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x802), player, false);
    game.world_mut()
        .set_actor_value(player, "onehanded", 10.0)
        .unwrap();
    let offer = training_menu::entry(&mut game, trainer).unwrap();
    assert_eq!(offer.trainer, trainer);
    assert_eq!(
        training_menu::heading(&offer),
        "onehanded: 10 / 25 (5 sessions left)"
    );
    assert_eq!(training_menu::purchase_label(&offer), "Train for 100 gold");
    assert!(training_menu::entry(&mut game, id(0x999)).is_none());
    assert_eq!(training_menu::UNAVAILABLE_LABEL, "Training is unavailable.");
}
