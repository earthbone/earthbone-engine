// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_godot_host::training_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x910).sub(T(b"EDID"), b"Gold001\0".to_vec()));
    let mut class = vec![0; 36];
    class[4] = 0;
    class[5] = 1;
    plugin.push_top(Record::new(T(b"CLAS"), 0x911).sub(T(b"DATA"), class));
    plugin.push_top(
        npc::build_record(
            0x912,
            "Trainer",
            "Trainer",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"CNAM"), 0x911_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x913)
            .sub(T(b"EDID"), b"fTrainingMultCost\0".to_vec())
            .sub(T(b"DATA"), 2.5_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("TrainingAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_16_adversarial_save_load_preserves_the_five_session_limit() {
    let mut original = game();
    let player = original.player();
    let trainer = original.world_mut().place_at(id(0x912), player, false);
    original
        .world_mut()
        .set_actor_value(player, "onehanded", 10.0)
        .unwrap();
    original.world_mut().add_item(player, id(0x910), 2_000);
    for _ in 0..2 {
        training_menu::purchase(&mut original, trainer).unwrap();
    }
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    let offer = training_menu::entry(&mut restored, trainer).unwrap();
    assert_eq!(offer.sessions_remaining, 3);
    assert_eq!(offer.current, 12.0);
}

#[test]
fn ui_16_adversarial_cost_rounds_up_and_invalid_trainer_has_no_offer() {
    let mut game = game();
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x912), player, false);
    game.world_mut()
        .set_actor_value(player, "onehanded", 11.0)
        .unwrap();
    game.world_mut().add_item(player, id(0x910), 100);
    assert_eq!(training_menu::entry(&mut game, trainer).unwrap().cost, 28);
    assert_eq!(training_menu::entry(&mut game, id(0x999)), None);
}
