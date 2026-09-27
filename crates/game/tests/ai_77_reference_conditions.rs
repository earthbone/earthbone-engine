// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-77 reference state CTDA contracts.

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
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    for object in [0x900_u32, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
    let game = Game::new(order);
    for (object, position) in [
        (0x900, [0.0, 0.0, 0.0]),
        (0x901, [3.0, 0.0, 0.0]),
        (0x902, [8.0, 0.0, 0.0]),
    ] {
        game.world_mut().reference(id(object)).unwrap().position = position;
    }
    game
}

fn evaluate(game: &Game, tested: u32, condition: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(tested),
            target: None,
            quest: None,
            tempering_enchanted: None,
        },
        condition,
    )
}

#[test]
fn ai_77_closer_to_a_than_b_uses_strict_world_distance() {
    let game = fixture();
    assert!(conditions::supports_function(577));
    assert_eq!(
        evaluate(&game, 0x900, &condition(577, 0x901, 0x902, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, 0x900, &condition(577, 0x902, 0x901, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, 0x900, &condition(577, 0x901, 0x902, 0.0)),
        Ok(false)
    );
}

#[test]
fn ai_77_closer_to_a_than_b_rejects_invalid_references() {
    let game = fixture();
    assert!(matches!(
        evaluate(&game, 0x999, &condition(577, 0x901, 0x902, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, 0x900, &condition(577, 0x999, 0x902, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, 0x900, &condition(577, 0x901, 0x800, 1.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_77_unmodeled_condition_state_returns_function_diagnostics() {
    let game = fixture();
    for function in [608, 611, 612, 631, 638, 642, 645] {
        assert!(!conditions::supports_function(function));
        for expected in [0.0, 1.0] {
            assert_eq!(
                evaluate(&game, 0x900, &condition(function, 0, 0, expected)),
                Err(conditions::Error::Function(function)),
                "function {function} with expected value {expected}"
            );
        }
    }
}
