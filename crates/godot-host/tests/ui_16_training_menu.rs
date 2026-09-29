// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-16 trainer menu request, offer and purchase behavior.

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
    Game::new(order)
}

#[test]
fn ui_16_papyrus_request_opens_the_trainer_offer_and_purchase_obeys_dlg_16() {
    let mut game = training_game();
    assert!(game.vm.natives.has_global("Game", "ShowTrainingMenu"));
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x802), player, false);
    game.world_mut()
        .set_actor_value(player, "onehanded", 10.0)
        .unwrap();
    game.world_mut().add_item(player, id(0x800), 2_000);

    assert!(game.world_mut().request_training_menu(trainer));
    assert_eq!(game.world_mut().take_training_menu_request(), Some(trainer));
    let offer = training_menu::entry(&mut game, trainer).unwrap();
    assert_eq!(offer.skill, "onehanded");
    assert_eq!(offer.current, 10.0);
    assert_eq!(offer.cap, 25.0);
    assert_eq!(offer.cost, 100);
    assert_eq!(offer.sessions_remaining, 5);

    assert_eq!(training_menu::purchase(&mut game, trainer), Ok(100));
    assert_eq!(game.world_mut().base_actor_value(player, "onehanded"), 11.0);
    assert_eq!(game.world_mut().item_count(player, id(0x800)), 1_900);
    assert_eq!(game.world_mut().item_count(trainer, id(0x800)), 100);
    assert_eq!(
        training_menu::entry(&mut game, trainer)
            .unwrap()
            .sessions_remaining,
        4
    );
}

#[test]
fn ui_16_menu_purchase_rejects_the_sixth_session_and_skill_tier_cap() {
    let mut game = training_game();
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x802), player, false);
    game.world_mut()
        .set_actor_value(player, "onehanded", 10.0)
        .unwrap();
    game.world_mut().add_item(player, id(0x800), 2_000);
    for _ in 0..5 {
        assert!(training_menu::purchase(&mut game, trainer).is_ok());
    }
    let offer = training_menu::entry(&mut game, trainer).unwrap();
    assert_eq!(offer.sessions_remaining, 0);
    assert_eq!(
        training_menu::purchase(&mut game, trainer),
        Err(soe_game::world::training::Error::Limit)
    );

    let mut capped = training_game();
    let player = capped.player();
    let trainer = capped.world_mut().place_at(id(0x802), player, false);
    capped
        .world_mut()
        .set_actor_value(player, "onehanded", 25.0)
        .unwrap();
    let offer = training_menu::entry(&mut capped, trainer).unwrap();
    assert_eq!(offer.current, offer.cap);
    assert_eq!(
        training_menu::purchase(&mut capped, trainer),
        Err(soe_game::world::training::Error::Limit)
    );
}
