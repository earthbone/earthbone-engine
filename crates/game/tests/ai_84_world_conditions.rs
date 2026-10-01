// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-84 calendar and world-state CTDA contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, first: u32, second: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [first, second],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(Record::new(Tag(*b"WTHR"), 0x801));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai84WorldConditions.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, c: &Condition) -> Result<bool, conditions::Error> {
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
        c,
    )
}

#[test]
fn ai_84_get_day_of_week_matches_calendar_and_opposite_comparison() {
    let game = fixture();
    game.world_mut().advance_clock(4320.0);

    assert!(conditions::supports_function(170));
    assert_eq!(evaluate(&game, &condition(170, 0, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(170, 0, 0, 0.0)), Ok(false));
}

#[test]
fn ai_84_world_functions_report_missing_contract_diagnostics() {
    let game = fixture();
    for function in [12, 62, 75, 148, 149, 153, 154] {
        assert!(!conditions::supports_function(function));
        let parameter = if function == 149 { 0x801 } else { 0 };
        for expected in [0.0, 1.0] {
            assert_eq!(
                evaluate(&game, &condition(function, parameter, 0, expected)),
                Err(conditions::Error::Function(function)),
                "function {function} has no usable local CK condition contract"
            );
        }
    }
}

#[test]
fn ai_84_world_conditions_validate_xedit_parameter_slots() {
    let game = fixture();
    for function in [12, 62, 75, 148, 153, 154, 170] {
        assert_eq!(
            evaluate(&game, &condition(function, 1, 0, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
        assert_eq!(
            evaluate(&game, &condition(function, 0, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    assert_eq!(
        evaluate(&game, &condition(149, 0x802, 0, 0.0)),
        Err(conditions::Error::Form(id(0x802)))
    );
}

#[test]
fn ai_84_world_conditions_reject_invalid_run_on_context() {
    let game = fixture();
    for function in [12, 62, 75, 148, 149, 153, 154, 170] {
        let mut c = condition(function, 0, 0, 0.0);
        c.run_on = 99;
        assert_eq!(
            evaluate(&game, &c),
            Err(conditions::Error::Target(99)),
            "function {function} must reject an unavailable run-on target"
        );
    }
}
