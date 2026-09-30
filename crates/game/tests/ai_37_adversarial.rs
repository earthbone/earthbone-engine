// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }
fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x820));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x821));
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0x830).sub(Tag(*b"KWDA"), 0x820u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("AI37Adversarial.esp", plugin).unwrap();
    Game::new(order)
}
fn condition(function: u16, parameters: [u32; 2], expected: f32) -> Condition {
    Condition { flags: 0, comparison: Comparison::Number(expected), function, parameters, run_on: 0, reference: 0, parameter_3: -1, strings: [None, None] }
}
fn eval(game: &Game, actor: GlobalFormId, c: Condition, event: Option<conditions::SkillUseContext>, light: Option<f32>) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(&mut game.world_mut(), &game.vm, conditions::Context { owner: id(0x800), subject: actor, target: None, quest: None, tempering_enchanted: None, skill_use: event, light_level: light }, &c)
}

#[test]
fn ai_37_actor_value_percent_uses_the_named_value_and_rejects_bad_context() {
    let game = game();
    game.world_mut().set_actor_value(id(0x900), "health", 100.0).unwrap();
    game.world_mut().damage_actor_value(id(0x900), "health", 20.0).unwrap();
    assert_eq!(eval(&game, id(0x900), condition(640, [24, 0], 0.8), None, None), Ok(true));
    assert_eq!(eval(&game, id(0x900), condition(640, [24, 1], 0.8), None, None), Err(conditions::Error::Parameter(640, 1)));
    assert_eq!(eval(&game, id(0x900), condition(640, [999, 0], 0.0), None, None), Err(conditions::Error::ActorValue(999)));
    assert_eq!(eval(&game, id(0x821), condition(640, [24, 0], 0.0), None, None), Err(conditions::Error::Form(id(0x821))));
}

#[test]
fn ai_37_skill_use_conditions_require_matching_event_and_compare_source_fields() {
    let game = game();
    let event = conditions::SkillUseContext { actor: id(0x900), action: 3, advance_object: Some(id(0x830)) };
    assert_eq!(eval(&game, id(0x900), condition(691, [0x820, 0], 1.0), Some(event), None), Ok(true));
    assert_eq!(eval(&game, id(0x900), condition(691, [0x821, 0], 1.0), Some(event), None), Ok(false));
    assert_eq!(eval(&game, id(0x900), condition(692, [3, 0], 1.0), Some(event), None), Ok(true));
    assert_eq!(eval(&game, id(0x900), condition(692, [4, 0], 1.0), Some(event), None), Ok(false));
    assert_eq!(eval(&game, id(0x900), condition(691, [0x820, 0], 1.0), None, None), Err(conditions::Error::SkillUseContext(691)));
}

#[test]
fn ai_37_light_and_warmth_contract_boundaries_are_diagnostic() {
    let game = game();
    assert_eq!(eval(&game, id(0x900), condition(711, [0, 0], 42.0), None, Some(42.0)), Ok(true));
    assert_eq!(eval(&game, id(0x900), condition(711, [0, 0], 0.0), None, Some(f32::NAN)), Err(conditions::Error::LightLevelContext(711)));
    assert_eq!(eval(&game, id(0x900), condition(711, [0, 0], 0.0), None, Some(151.0)), Err(conditions::Error::LightLevelContext(711)));
    assert_eq!(eval(&game, id(0x900), condition(735, [0, 0], 0.0), None, None), Err(conditions::Error::UnavailableContract(735)));
}

#[test]
fn ai_37_over_encumbered_is_strictly_greater_than_carry_weight() {
    let game = game();
    game.world_mut().set_actor_value(id(0x900), "inventoryweight", 100.0).unwrap();
    game.world_mut().set_actor_value(id(0x900), "carryweight", 100.0).unwrap();
    assert_eq!(eval(&game, id(0x900), condition(734, [0, 0], 1.0), None, None), Ok(false));
    game.world_mut().set_actor_value(id(0x900), "inventoryweight", 100.01).unwrap();
    assert_eq!(eval(&game, id(0x900), condition(734, [0, 0], 1.0), None, None), Ok(true));
}
