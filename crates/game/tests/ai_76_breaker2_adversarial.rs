// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for (tag, object) in [
        (*b"WOOP", 0xA00),
        (*b"MGEF", 0xA01),
        (*b"ENCH", 0xA02),
        (*b"SHOU", 0xA03),
    ] {
        plugin.push_top(Record::new(Tag(tag), object));
    }
    let mut order = LoadOrder::new();
    order.add("Ai76Breaker2.esm", plugin).unwrap();
    Game::new(order)
}
fn eval(game: &Game, form: u32, expected: f32) -> Result<bool, conditions::Error> {
    let c = Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 493,
        parameters: [form, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(0x900),
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        &c,
    )
}

#[test]
fn ai_76_player_knows_word_of_power_uses_known_flag() {
    let game = game();
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .known_forms
        .insert(id(0xA00));
    assert_eq!(eval(&game, 0xA00, 1.0), Ok(true));
    assert_eq!(eval(&game, 0xA00, 0.0), Ok(false));
}

#[test]
fn ai_76_player_knows_rejects_shout_form_type() {
    let game = game();
    assert_eq!(
        eval(&game, 0xA03, 1.0),
        Err(conditions::Error::StateContract(493))
    );
}

#[test]
fn ai_76_known_effect_state_survives_save_load_and_remains_per_form() {
    let source_game = game();
    let player = source_game.world_mut().player();
    source_game
        .world_mut()
        .actor(player)
        .unwrap()
        .known_forms
        .insert(id(0xA01));
    let bytes = source_game.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&bytes).unwrap();
    assert_eq!(eval(&restored, 0xA01, 1.0), Ok(true));
    assert_eq!(eval(&restored, 0xA00, 1.0), Ok(false));
}
