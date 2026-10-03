// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based adversarial checks for AI-85 world conditions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"WATR"), 0x812));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x912).sub(Tag(*b"NAME"), 0x812_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x913));
    let mut order = LoadOrder::new();
    order.add("Ai85Breaker2.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
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
        c,
    )
}

#[test]
fn ai_85_restrained_and_unconscious_states_are_mutually_exclusive_for_queries() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.restrained = true;
        actor.unconscious = false;
    }
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(true));

    game.world_mut().actor(id(0x900)).unwrap().unconscious = true;
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 0.0)), Ok(true));

    game.world_mut().actor(id(0x900)).unwrap().restrained = false;
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 0.0)), Ok(true));
}

#[test]
fn ai_85_real_hours_are_fractional_idempotent_and_independent_of_timescale() {
    let game = fixture();
    game.world_mut().clock.timescale = 30.0;
    game.world_mut().advance_clock(1_800.0);
    let half_hour = condition(716, 0.5);
    assert_eq!(evaluate(&game, id(0x900), &half_hour), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &half_hour), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.5001)), Ok(false));
}

#[test]
fn ai_85_real_hours_continue_after_multiple_engine_save_loads() {
    let first = fixture();
    first.world_mut().advance_clock(3_600.0);
    let first_save = first.save_state().unwrap();

    let mut second = fixture();
    second.load_state(&first_save).unwrap();
    assert_eq!(evaluate(&second, id(0x900), &condition(716, 1.0)), Ok(true));
    second.world_mut().advance_clock(1_800.0);
    let second_save = second.save_state().unwrap();

    let mut third = fixture();
    third.load_state(&second_save).unwrap();
    assert_eq!(evaluate(&third, id(0x900), &condition(716, 1.5)), Ok(true));
    assert_eq!(evaluate(&third, id(0x900), &condition(716, 2.0)), Ok(false));
}

#[test]
fn ai_85_water_object_rejects_references_without_a_valid_water_base() {
    let game = fixture();
    assert_eq!(evaluate(&game, id(0x912), &condition(304, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x910), &condition(304, 1.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x913), &condition(304, 1.0)), Err(conditions::Error::Form(id(0x913))));
    assert_eq!(evaluate(&game, id(0x812), &condition(304, 1.0)), Err(conditions::Error::Form(id(0x812))));
}

#[test]
fn ai_85_parameterless_implemented_functions_reject_each_parameter_slot() {
    let game = fixture();
    for function in [244, 304, 716] {
        for slot in 0..2 {
            let mut query = condition(function, 0.0);
            query.parameters[slot] = 1;
            assert_eq!(
                evaluate(&game, id(0x900), &query),
                Err(conditions::Error::Parameter(function, 1)),
                "function {function}, slot {slot}"
            );
        }
    }
}

#[test]
fn ai_85_functions_without_behavior_contracts_keep_explicit_diagnostics() {
    let game = fixture();
    for function in [255, 265, 266, 267, 332] {
        assert!(!conditions::supports_function(function));
        for expected in [0.0, 1.0] {
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, expected)),
                Err(conditions::Error::Function(function)),
                "function {function}, expected {expected}"
            );
        }
    }
}
