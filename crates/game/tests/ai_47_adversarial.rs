// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }
fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"SCEN"), 0xA00));
    let mut order = LoadOrder::new();
    order.add("AI47Adversarial.esp", plugin).unwrap();
    Game::new(order)
}
fn c(function: u16, params: [u32; 2], expected: f32) -> Condition {
    Condition { flags: 0, comparison: Comparison::Number(expected), function, parameters: params, run_on: 0, reference: 0, parameter_3: -1, strings: [None, None] }
}
fn eval(game: &Game, subject: GlobalFormId, condition: Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(&mut game.world_mut(), &game.vm, conditions::Context { owner: id(0x800), subject, target: None, quest: None, tempering_enchanted: None, skill_use: None, light_level: None }, &condition)
}

#[test]
fn ai_47_unsupported_talk_counters_and_talking_predicate_fail_explicitly() {
    let game = game();
    for function in [50, 141, 172] {
        assert!(!conditions::supports_function(function));
        assert_eq!(eval(&game, id(0x900), c(function, [0, 0], 0.0)), Err(conditions::Error::Function(function)));
    }
}

#[test]
fn ai_47_greeting_requires_the_same_actor_and_no_parameter() {
    let game = game();
    assert_eq!(eval(&game, id(0x900), c(123, [0, 0], 1.0)), Ok(false));
    assert_eq!(eval(&game, id(0x800), c(123, [0, 0], 1.0)), Ok(false));
    assert_eq!(eval(&game, id(0x900), c(123, [1, 0], 1.0)), Err(conditions::Error::Parameter(123, 1)));
}

#[test]
fn ai_47_persuasion_functions_use_current_speechcraft_and_reject_parameters() {
    let game = game();
    game.world_mut().set_actor_value(id(0x900), "speechcraft", 37.0).unwrap();
    for function in [225, 315] {
        assert_eq!(eval(&game, id(0x900), c(function, [0, 0], 37.0)), Ok(true));
        assert_eq!(eval(&game, id(0x900), c(function, [1, 0], 37.0)), Err(conditions::Error::Parameter(function, 1)));
        assert_eq!(eval(&game, id(0x800), c(function, [0, 0], 0.0)), Err(conditions::Error::Form(id(0x800))));
    }
}

#[test]
fn ai_47_scene_and_dialogue_require_valid_identity_and_active_player_pair() {
    let mut game = game();
    assert_eq!(eval(&game, id(0x900), c(248, [0xA00, 0], 1.0)), Ok(false));
    game.world_mut().scenes.insert(id(0xA00));
    assert_eq!(eval(&game, id(0x900), c(248, [0xA00, 0], 1.0)), Ok(true));
    assert_eq!(eval(&game, id(0x900), c(248, [0xA00, 1], 1.0)), Err(conditions::Error::Parameter(248, 1)));
    let player = game.world_mut().player();
    game.begin_dialogue(id(0x900), player).unwrap();
    assert_eq!(eval(&game, id(0x900), c(249, [0, 0], 1.0)), Ok(true));
    game.close_dialogue();
    assert_eq!(eval(&game, id(0x900), c(249, [0, 0], 1.0)), Ok(false));
}
