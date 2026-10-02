// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-53 legacy condition functions with missing and available contracts.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, ai, conditions, package};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai53.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, parameters: [u32; 2], expected: f32, run_on: u32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters,
        run_on,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
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
            light_level: None,
            skill_use: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        c,
    )
}

#[test]
fn ai_53_is_ps3_matches_non_playstation_platform_and_parameter_contract() {
    let game = fixture();
    assert!(conditions::supports_function(520));

    let platform_match = condition(520, [0, 0], 0.0, 0);
    assert_eq!(evaluate(&game, &platform_match), Ok(true));
    let platform_mismatch = condition(520, [0, 0], 1.0, 0);
    assert_eq!(evaluate(&game, &platform_mismatch), Ok(false));
    assert!(ai::conditions_hold(
        &mut game.world_mut(),
        id(0x900),
        id(0x800),
        None,
        &[package::Condition {
            comparison: 0,
            flags: 0,
            value: package::CompValue::Float(0.0),
            function: 520,
            param1: 0,
            param2: 0,
            run_on: 0,
            reference: 0,
            param3: -1,
        }],
    ));

    for parameters in [[1, 0], [0, 1]] {
        let malformed = condition(520, parameters, 0.0, 0);
        assert_eq!(
            evaluate(&game, &malformed),
            Err(conditions::Error::Parameter(520, 1))
        );
    }
    let invalid_context = condition(520, [0, 0], 0.0, 1);
    assert_eq!(
        evaluate(&game, &invalid_context),
        Err(conditions::Error::Target(1))
    );
}

#[test]
fn ai_53_vats_conditions_report_unsupported_contract_and_invalid_context() {
    let game = fixture();
    for function in [516, 517, 518, 522, 523, 524, 525] {
        assert!(!conditions::supports_function(function), "{function}");
        for expected in [0.0, 1.0] {
            let unsupported = condition(function, [0x910, 0], expected, 0);
            assert_eq!(
                evaluate(&game, &unsupported),
                Err(conditions::Error::Function(function)),
                "{function}, expected {expected}"
            );
        }
        let invalid_context = condition(function, [0x910, 0], 1.0, 1);
        assert_eq!(
            evaluate(&game, &invalid_context),
            Err(conditions::Error::Target(1)),
            "{function}"
        );

        let mut world = game.world_mut();
        assert!(!ai::conditions_hold(
            &mut world,
            id(0x900),
            id(0x800),
            None,
            &[package::Condition {
                comparison: 0,
                flags: 0,
                value: package::CompValue::Float(1.0),
                function,
                param1: 0x910,
                param2: 0,
                run_on: 0,
                reference: 0,
                param3: -1,
            }],
        ));
        assert_eq!(world.ai.unsupported_functions.get(&function), Some(&1));
    }
}
