// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-72 source-defined condition behavior.

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
const DOOR: u32 = 0x810;
const OTHER_REF: u32 = 0x811;
const PACKAGE: u32 = 0x820;
const OTHER_PACKAGE: u32 = 0x821;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
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
    plugin.push_top(Record::new(Tag(*b"REFR"), DOOR).sub(Tag(*b"NAME"), DOOR_BASE.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), OTHER_REF).sub(Tag(*b"NAME"), OTHER_BASE.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("AI72Breaker.esm", plugin).unwrap();
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

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    predicate: &Condition,
) -> Result<bool, conditions::Error> {
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
        predicate,
    )
}

fn package_evaluate(game: &Game, subject: GlobalFormId, predicate: &Condition) -> bool {
    let Comparison::Number(expected) = predicate.comparison else {
        panic!("fixture requires numeric comparison");
    };
    ai::conditions_hold(
        &mut game.world_mut(),
        subject,
        id(OWNER),
        None,
        &[package::Condition {
            comparison: predicate.operator(),
            flags: 0,
            value: package::CompValue::Float(expected),
            function: predicate.function,
            param1: predicate.parameters[0],
            param2: predicate.parameters[1],
            run_on: predicate.run_on,
            reference: predicate.reference,
            param3: predicate.parameter_3,
        }],
    )
}

#[test]
fn ck_open_state_covers_none_open_opening_closed_and_closing() {
    let game = fixture();
    let door = id(DOOR);
    assert_eq!(
        evaluate(&game, id(OTHER_REF), &condition(157, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(OTHER_REF), &condition(157, 0, 3.0)),
        Ok(false)
    );

    for (state, expected) in [(1, true), (3, false)] {
        game.world_mut().reference(door).unwrap().opened = state == 1;
        assert_eq!(
            evaluate(&game, door, &condition(157, 0, state as f32)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                door,
                &condition(157, 0, if expected { 3.0 } else { 1.0 })
            ),
            Ok(false)
        );
    }
    assert!(game.world_mut().begin_door_opening(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 2.0)), Ok(true));
    assert!(game.world_mut().finish_door_transition(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 1.0)), Ok(true));
    assert!(game.world_mut().begin_door_closing(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 4.0)), Ok(true));
    assert!(game.world_mut().finish_door_transition(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 3.0)), Ok(true));
    assert!(!game.world_mut().begin_door_opening(id(OTHER_REF)));
    assert!(!game.world_mut().begin_door_opening(id(0xffff)));
    assert!(!game.world_mut().finish_door_transition(door));
}

#[test]
fn open_state_rejects_deleted_missing_and_parameterized_contexts() {
    let game = fixture();
    let door = id(DOOR);
    for invalid in [id(0xffff)] {
        assert!(matches!(
            evaluate(&game, invalid, &condition(157, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    game.world_mut().reference(door).unwrap().deleted = true;
    assert_eq!(
        evaluate(&game, door, &condition(157, 0, 0.0)),
        Err(conditions::Error::Form(door))
    );
    let mut parameterized = condition(157, 0, 0.0);
    parameterized.parameters[0] = 1;
    assert_eq!(
        evaluate(&game, id(OTHER_REF), &parameterized),
        Err(conditions::Error::Parameter(157, 1))
    );
}

#[test]
fn locking_during_opening_snaps_the_reported_state_closed() {
    let mut game = fixture();
    let door = id(DOOR);
    game.object_of(soe_game::Handle::Form(door));
    assert!(game.world_mut().begin_door_opening(door));

    game.world_mut()
        .set_locked(door, true, &mut |_, _| panic!("unexpected form allocation"));

    assert_eq!(evaluate(&game, door, &condition(157, 0, 3.0)), Ok(true));
}

#[test]
fn settled_door_state_survives_engine_save_load() {
    let game = fixture();
    let door = id(DOOR);
    assert!(game.world_mut().begin_door_opening(door));
    assert!(game.world_mut().finish_door_transition(door));

    let saved = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();

    assert_eq!(evaluate(&restored, door, &condition(157, 0, 1.0)), Ok(true));
}

#[test]
fn current_package_requires_actor_and_matches_only_the_selected_package() {
    let game = fixture();
    game.world_mut().actor(id(ACTOR)).unwrap().ai.package = Some(id(PACKAGE));
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, PACKAGE, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, OTHER_PACKAGE, 0.0)),
        Ok(true)
    );
    game.world_mut().actor(id(ACTOR)).unwrap().ai.package = None;
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, PACKAGE, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), &condition(161, PACKAGE, 1.0)),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, id(DOOR), &condition(161, PACKAGE, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(ACTOR), &condition(161, 0xffff, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    let mut invalid_parameter = condition(161, PACKAGE, 0.0);
    invalid_parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(ACTOR), &invalid_parameter),
        Err(conditions::Error::Parameter(161, 1))
    );
}

#[test]
fn destroyed_and_default_open_report_positive_negative_and_deleted_states() {
    let game = fixture();
    let door = id(DOOR);
    assert_eq!(evaluate(&game, door, &condition(203, 0, 0.0)), Ok(true));
    game.world_mut().reference(door).unwrap().destroyed = true;
    assert_eq!(evaluate(&game, door, &condition(203, 0, 1.0)), Ok(true));
    game.world_mut().reference(door).unwrap().destroyed = false;
    assert_eq!(evaluate(&game, door, &condition(203, 0, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, door, &condition(215, 0, 0.0)), Ok(true));

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"DOOR"), 0x803));
    let open_record = Record::new(Tag(*b"REFR"), 0x812)
        .sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes())
        .sub(Tag(*b"ONAM"), Vec::<u8>::new())
        .sub(Tag(*b"XACT"), 13_u32.to_le_bytes());
    plugin.push_top(open_record);
    let mut order = LoadOrder::new();
    order.add("DefaultOpen.esm", plugin).unwrap();
    let open_game = Game::new(order);
    assert_eq!(
        evaluate(&open_game, id(0x812), &condition(215, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&open_game, id(0x812), &condition(203, 0, 0.0)),
        Ok(true)
    );
    open_game.world_mut().reference(id(0x812)).unwrap().deleted = true;
    for function in [203, 215] {
        assert_eq!(
            evaluate(&open_game, id(0x812), &condition(function, 0, 0.0)),
            Err(conditions::Error::Form(id(0x812)))
        );
    }
}

#[test]
fn undocumented_queries_return_explicit_diagnostics_for_any_subject() {
    let game = fixture();
    for function in [143, 150, 226, 227] {
        assert!(!conditions::supports_function(function));
        for subject in [id(ACTOR), id(DOOR), id(0xffff)] {
            assert_eq!(
                evaluate(&game, subject, &condition(function, 0, 0.0)),
                Err(conditions::Error::Function(function))
            );
        }
    }
}

#[test]
fn package_evaluator_matches_reference_state_queries() {
    let game = fixture();
    let door = id(DOOR);
    assert!(game.world_mut().begin_door_opening(door));
    assert!(package_evaluate(&game, door, &condition(157, 0, 2.0)));
    assert!(!package_evaluate(&game, door, &condition(157, 0, 1.0)));
    assert!(game.world_mut().finish_door_transition(door));
    game.world_mut().reference(door).unwrap().destroyed = true;
    assert!(package_evaluate(&game, door, &condition(203, 0, 1.0)));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.package = Some(id(PACKAGE));
    assert!(package_evaluate(
        &game,
        id(ACTOR),
        &condition(161, PACKAGE, 1.0)
    ));
}
