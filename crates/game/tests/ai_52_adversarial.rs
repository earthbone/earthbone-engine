// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! AI-52 legacy CTDA functions with no usable local CK contract.

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
    order.add("Ai52.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, parameter: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [parameter, 0],
        run_on: 0,
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

fn package_condition(
    function: u16,
    parameter: u32,
    comparison: u8,
    expected: f32,
) -> package::Condition {
    package::Condition {
        comparison,
        flags: 0,
        value: package::CompValue::Float(expected),
        function,
        param1: parameter,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    }
}

#[test]
fn ai_52_is_xbox_is_false_for_pc_conditions_and_package_conditions() {
    let game = fixture();
    assert!(conditions::supports_function(309));
    assert_eq!(evaluate(&game, &condition(309, 0, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(309, 0, 1.0)), Ok(false));

    let matches_pc = package_condition(309, 0, package::condition::EQUAL, 0.0);
    let matches_xbox = package_condition(309, 0, package::condition::EQUAL, 1.0);
    assert!(ai::conditions_hold(
        &mut game.world_mut(),
        id(0x900),
        id(0x800),
        None,
        &[matches_pc]
    ));
    assert!(!ai::conditions_hold(
        &mut game.world_mut(),
        id(0x900),
        id(0x800),
        None,
        &[matches_xbox]
    ));
}

#[test]
fn ai_52_is_xbox_rejects_unused_parameters_and_missing_run_on_context() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, &condition(309, 1, 0.0)),
        Err(conditions::Error::Parameter(309, 1))
    );

    let mut no_target = condition(309, 0, 0.0);
    no_target.run_on = 1;
    assert_eq!(
        evaluate(&game, &no_target),
        Err(conditions::Error::Target(1))
    );

    let invalid_package_condition = package_condition(309, 1, package::condition::EQUAL, 0.0);
    assert!(!ai::conditions_hold(
        &mut game.world_mut(),
        id(0x900),
        id(0x800),
        None,
        &[invalid_package_condition]
    ));
    assert_eq!(
        game.world_mut().ai.unsupported_functions.get(&309),
        Some(&1)
    );
}

#[test]
fn ai_52_legacy_conditions_diagnose_missing_contracts_and_invalid_context() {
    let game = fixture();
    for function in [224, 235, 381, 407, 427, 473, 515] {
        assert!(!conditions::supports_function(function), "{function}");
        assert_eq!(
            evaluate(&game, &condition(function, 0, 0.0)),
            Err(conditions::Error::Function(function)),
            "{function}"
        );

        let mut no_target = condition(function, 0, 0.0);
        no_target.run_on = 1;
        assert_eq!(
            evaluate(&game, &no_target),
            Err(conditions::Error::Target(1)),
            "{function}"
        );

        let package_predicate = package_condition(function, 0, package::condition::EQUAL, 0.0);
        assert!(!ai::conditions_hold(
            &mut game.world_mut(),
            id(0x900),
            id(0x800),
            None,
            &[package_predicate]
        ));
        assert_eq!(
            game.world_mut().ai.unsupported_functions.get(&function),
            Some(&1),
            "{function}"
        );
    }
}

#[test]
fn unsupported_legacy_condition_is_diagnosed_once_per_evaluation() {
    let game = fixture();
    for _ in 0..3 {
        assert_eq!(
            evaluate(&game, &condition(407, 0, 0.0)),
            Err(conditions::Error::Function(407))
        );
    }
}
