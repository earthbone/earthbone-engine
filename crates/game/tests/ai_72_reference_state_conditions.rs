// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-72 condition queries for reference state and package identity.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{ai, conditions, package, Game};

const OWNER: u32 = 0x800;
const ACTOR: u32 = 0x801;
const ACTOR_BASE: u32 = 0x802;
const DOOR_BASE: u32 = 0x803;
const OTHER_BASE: u32 = 0x804;
const CLOSED_DOOR: u32 = 0x810;
const OPEN_DOOR: u32 = 0x811;
const OTHER_REF: u32 = 0x812;
const PACKAGE: u32 = 0x820;
const OTHER_PACKAGE: u32 = 0x821;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(form: u32, base: u32, default_open: bool) -> Record {
    let mut record = Record::new(Tag(*b"REFR"), form).sub(Tag(*b"NAME"), base.to_le_bytes());
    if default_open {
        record = record
            .sub(Tag(*b"ONAM"), Vec::<u8>::new())
            .sub(Tag(*b"XACT"), 13_u32.to_le_bytes());
    }
    record
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), OWNER));
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR_BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), ACTOR_BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"DOOR"), DOOR_BASE));
    plugin.push_top(Record::new(Tag(*b"STAT"), OTHER_BASE));
    plugin.push_top(Record::new(Tag(*b"PACK"), PACKAGE));
    plugin.push_top(Record::new(Tag(*b"PACK"), OTHER_PACKAGE));
    plugin.push_top(reference(CLOSED_DOOR, DOOR_BASE, false));
    plugin.push_top(reference(OPEN_DOOR, DOOR_BASE, true));
    plugin.push_top(reference(OTHER_REF, OTHER_BASE, false));
    let mut order = LoadOrder::new();
    order.add("AI72.esm", plugin).unwrap();
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

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(OWNER),
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

fn package_evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> bool {
    let Comparison::Number(expected) = c.comparison else {
        panic!("fixture requires a numeric comparison");
    };
    ai::conditions_hold(
        &mut game.world_mut(),
        subject,
        id(OWNER),
        None,
        &[package::Condition {
            comparison: c.operator(),
            flags: 0,
            value: package::CompValue::Float(expected),
            function: c.function,
            param1: c.parameters[0],
            param2: c.parameters[1],
            run_on: c.run_on,
            reference: c.reference,
            param3: -1,
        }],
    )
}

#[test]
fn ai_72_open_state_and_default_open_follow_reference_records() {
    let game = fixture();
    for function in [157, 215] {
        assert!(conditions::supports_function(function));
    }

    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(157, 0, 3.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(OPEN_DOOR), &condition(157, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(OTHER_REF), &condition(157, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(OPEN_DOOR), &condition(215, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(215, 0, 0.0)),
        Ok(true)
    );
    assert!(package_evaluate(
        &game,
        id(CLOSED_DOOR),
        &condition(157, 0, 3.0)
    ));
    assert!(package_evaluate(
        &game,
        id(OPEN_DOOR),
        &condition(157, 0, 1.0)
    ));
    assert!(package_evaluate(
        &game,
        id(OPEN_DOOR),
        &condition(215, 0, 1.0)
    ));
    assert!(package_evaluate(
        &game,
        id(CLOSED_DOOR),
        &condition(215, 0, 0.0)
    ));

    game.world_mut().reference(id(CLOSED_DOOR)).unwrap().opened = true;
    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(157, 0, 1.0)),
        Ok(true)
    );
    assert!(package_evaluate(
        &game,
        id(CLOSED_DOOR),
        &condition(157, 0, 1.0)
    ));
    for invalid in [id(0xffff), id(ACTOR_BASE)] {
        assert!(matches!(
            evaluate(&game, invalid, &condition(157, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    assert_eq!(
        evaluate(&game, id(OPEN_DOOR), &condition(157, 1, 1.0)),
        Err(conditions::Error::Parameter(157, 1))
    );
    for invalid in [id(0xffff), id(ACTOR_BASE)] {
        assert!(matches!(
            evaluate(&game, invalid, &condition(215, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    assert_eq!(
        evaluate(&game, id(OPEN_DOOR), &condition(215, 1, 1.0)),
        Err(conditions::Error::Parameter(215, 1))
    );
}

#[test]
fn ai_72_package_conditions_report_opening_and_closing_transitions() {
    let game = fixture();
    let door = id(CLOSED_DOOR);

    assert!(game.world_mut().begin_door_opening(door));
    assert!(package_evaluate(&game, door, &condition(157, 0, 2.0)));
    assert!(!package_evaluate(&game, door, &condition(157, 0, 1.0)));
    assert!(game.world_mut().finish_door_transition(door));
    assert!(package_evaluate(&game, door, &condition(157, 0, 1.0)));

    assert!(game.world_mut().begin_door_closing(door));
    assert!(package_evaluate(&game, door, &condition(157, 0, 4.0)));
    assert!(!package_evaluate(&game, door, &condition(157, 0, 3.0)));
    assert!(game.world_mut().finish_door_transition(door));
    assert!(package_evaluate(&game, door, &condition(157, 0, 3.0)));
}

#[test]
fn ai_72_current_package_and_destroyed_queries_read_live_state() {
    let game = fixture();
    assert!(conditions::supports_function(161));
    assert!(conditions::supports_function(203));

    game.world_mut().actor(id(ACTOR)).unwrap().ai.package = Some(id(PACKAGE));
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, PACKAGE, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, OTHER_PACKAGE, 0.0)),
        Ok(true)
    );
    assert!(package_evaluate(
        &game,
        id(ACTOR),
        &condition(161, PACKAGE, 1.0)
    ));
    assert!(package_evaluate(
        &game,
        id(ACTOR),
        &condition(161, OTHER_PACKAGE, 0.0)
    ));
    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(161, PACKAGE, 0.0)),
        Err(conditions::Error::Form(id(CLOSED_DOOR)))
    );
    assert!(matches!(
        evaluate(&game, id(ACTOR), &condition(161, OTHER_BASE, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(!package_evaluate(
        &game,
        id(ACTOR),
        &condition(161, OTHER_PACKAGE, 1.0)
    ));
    let mut invalid_parameter = condition(161, PACKAGE, 0.0);
    invalid_parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(ACTOR), &invalid_parameter),
        Err(conditions::Error::Parameter(161, 1))
    );

    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(203, 0, 0.0)),
        Ok(true)
    );
    assert!(package_evaluate(
        &game,
        id(CLOSED_DOOR),
        &condition(203, 0, 0.0)
    ));
    game.world_mut()
        .reference(id(CLOSED_DOOR))
        .unwrap()
        .destroyed = true;
    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(203, 0, 1.0)),
        Ok(true)
    );
    assert!(package_evaluate(
        &game,
        id(CLOSED_DOOR),
        &condition(203, 0, 1.0)
    ));
    assert!(matches!(
        evaluate(&game, id(0xffff), &condition(203, 0, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert_eq!(
        evaluate(&game, id(CLOSED_DOOR), &condition(203, 1, 1.0)),
        Err(conditions::Error::Parameter(203, 1))
    );
}

#[test]
fn ai_72_undocumented_queries_report_explicit_diagnostics() {
    let game = fixture();
    for function in [143, 150, 226, 227] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(ACTOR), &condition(function, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
        assert_eq!(
            evaluate(&game, id(0xffff), &condition(function, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}
