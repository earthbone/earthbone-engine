// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-16 trainer payment, skill and use-limit behavior.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
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
    );
    let trainer = trainer.sub(T(b"CNAM"), 0x801u32.to_le_bytes());
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

fn trainer(game: &mut Game) -> GlobalFormId {
    let player = game.player();
    game.world_mut().place_at(id(0x802), player, false)
}

fn buy(game: &mut Game, trainer: GlobalFormId) -> Result<i32, soe_game::world::training::Error> {
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    game.world_mut().purchase_training(trainer, &mut allocate)
}

#[test]
fn dlg_16_valid_purchase_debits_gold_and_raises_taught_skill_once() {
    let mut game = game();
    let player = game.player();
    let trainer = trainer(&mut game);
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 10.0).unwrap();
        world.add_item(player, id(0x800), 500);
    }

    assert_eq!(buy(&mut game, trainer), Ok(100));
    let mut world = game.world_mut();
    assert_eq!(world.base_actor_value(player, "onehanded"), 11.0);
    assert_eq!(world.base_actor_value(player, "twohanded"), 0.0);
    assert_eq!(world.item_count(player, id(0x800)), 400);
    assert_eq!(world.item_count(trainer, id(0x800)), 100);
}

#[test]
fn dlg_16_trainer_tier_cap_rejects_without_payment() {
    let mut game = game();
    let player = game.player();
    let trainer = trainer(&mut game);
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 25.0).unwrap();
        world.add_item(player, id(0x800), 500);
    }

    assert_eq!(
        buy(&mut game, trainer),
        Err(soe_game::world::training::Error::Limit)
    );
    let mut world = game.world_mut();
    assert_eq!(world.base_actor_value(player, "onehanded"), 25.0);
    assert_eq!(world.item_count(player, id(0x800)), 500);
}

#[test]
fn dlg_16_exhausted_level_limit_rejects_without_changing_gold_or_skill() {
    let mut game = game();
    let player = game.player();
    let trainer = trainer(&mut game);
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 10.0).unwrap();
        world.add_item(player, id(0x800), 2_000);
    }
    for _ in 0..5 {
        assert!(buy(&mut game, trainer).is_ok());
    }
    let (skill, gold) = {
        let mut world = game.world_mut();
        (
            world.base_actor_value(player, "onehanded"),
            world.item_count(player, id(0x800)),
        )
    };

    assert_eq!(
        buy(&mut game, trainer),
        Err(soe_game::world::training::Error::Limit)
    );
    let mut world = game.world_mut();
    assert_eq!(world.base_actor_value(player, "onehanded"), skill);
    assert_eq!(world.item_count(player, id(0x800)), gold);
}

#[test]
fn dlg_16_exhausted_limit_persists_across_world_save_load() {
    let mut game = game();
    let player = game.player();
    let trainer = trainer(&mut game);
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 10.0).unwrap();
        world.add_item(player, id(0x800), 2_000);
    }
    for _ in 0..5 {
        assert!(buy(&mut game, trainer).is_ok());
    }
    let saved = game.world_mut().save_state();
    game.world_mut().load_state(&saved).unwrap();

    assert_eq!(
        buy(&mut game, trainer),
        Err(soe_game::world::training::Error::Limit)
    );
}
