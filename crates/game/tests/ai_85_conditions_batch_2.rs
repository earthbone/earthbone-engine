// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-85 condition functions 244, 255, 265, 266, 267, 304, 332 and 716.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai85.esm", plugin).unwrap();
    Game::new(order)
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
fn ai_85_get_restrained_matches_actor_state_and_unconscious_exclusion() {
    let game = fixture();
    assert!(conditions::supports_function(244));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(false));
    game.world_mut().actor(id(0x900)).unwrap().restrained = true;
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 0.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(true));
    game.world_mut().actor(id(0x900)).unwrap().unconscious = true;
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 1.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 0.0)), Ok(true));
}

#[test]
fn ai_85_get_restrained_rejects_parameters_and_invalid_actor_context() {
    let game = fixture();
    let mut parameter = condition(244, 1.0);
    parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &parameter),
        Err(conditions::Error::Parameter(244, 1))
    );
    parameter.parameters = [1, 0];
    assert_eq!(
        evaluate(&game, id(0x900), &parameter),
        Err(conditions::Error::Parameter(244, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(244, 1.0)),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_85_get_real_hours_passed_uses_real_clock_hours() {
    let game = fixture();
    assert!(conditions::supports_function(716));
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.0)), Ok(true));
    game.world_mut().advance_clock(5_400.0);
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 1.5)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 2.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.0)), Ok(false));
}

#[test]
fn ai_85_parameterless_time_query_rejects_parameters_and_missing_run_on_target() {
    let game = fixture();
    let mut parameter = condition(716, 0.0);
    parameter.parameters[0] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &parameter),
        Err(conditions::Error::Parameter(716, 1))
    );
    parameter.parameters = [0, 1];
    assert_eq!(
        evaluate(&game, id(0x900), &parameter),
        Err(conditions::Error::Parameter(716, 1))
    );
    let mut missing_target = condition(716, 0.0);
    missing_target.run_on = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &missing_target),
        Err(conditions::Error::Target(1))
    );
}

#[test]
fn ai_85_functions_without_a_usable_contract_return_diagnostics() {
    let game = fixture();
    for function in [255, 265, 266, 267, 332] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 1.0)),
            Err(conditions::Error::Function(function)),
            "function {function}"
        );
    }
}
