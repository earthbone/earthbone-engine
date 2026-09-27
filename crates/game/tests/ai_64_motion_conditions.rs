// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-64 actor motion and animation condition contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const OTHER_ACTOR: u32 = 0x802;
const REFERENCE: u32 = 0x803;
const NON_ACTOR: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    for actor in [ACTOR, OTHER_ACTOR] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag(*b"BOOK"), NON_ACTOR));
    let mut order = LoadOrder::new();
    order.add("Ai64.esm", plugin).unwrap();
    Game::new(order)
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

fn evaluate(game: &Game, subject: u32, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(BASE),
            subject: id(subject),
            target: None,
            quest: None,
            tempering_enchanted: None,
        },
        c,
    )
}

fn set_condition_value(game: &Game, function: u16, value: bool) {
    let mut world = game.world_mut();
    if function == 339 {
        world.last_ridden_horse = value.then_some(id(ACTOR));
        return;
    }
    let actor = world.actor(id(ACTOR)).unwrap();
    match function {
        185 => actor.swimming = value,
        286 => actor.sneaking = value,
        287 => actor.running = value,
        327 => actor.riding_mount = value,
        _ => unreachable!(),
    }
}

#[test]
fn ai_64_actor_motion_conditions_match_ck_state_and_validate_context() {
    let game = fixture();
    for function in [185, 286, 287, 327, 339] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 0.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 1.0)),
            Ok(false)
        );
        set_condition_value(&game, function, true);
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 0.0)),
            Ok(false)
        );
        set_condition_value(&game, function, false);
        assert_eq!(
            evaluate(&game, NON_ACTOR, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(NON_ACTOR)))
        );
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 1, 0, 1.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 1, 1.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    assert_eq!(
        evaluate(&game, OTHER_ACTOR, &condition(339, 0, 0, 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_64_actor_motion_flags_survive_world_save_load() {
    let mut game = fixture();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.swimming = true;
        actor.running = true;
        actor.riding_mount = true;
    }
    let bytes = game.save_state().unwrap();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.swimming = false;
        actor.running = false;
        actor.riding_mount = false;
    }
    game.load_state(&bytes).unwrap();
    for function in [185, 287, 327] {
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 1.0)),
            Ok(true)
        );
    }
}

#[test]
fn ai_64_undocumented_animation_conditions_report_unavailable_after_validation() {
    let game = fixture();
    for function in [219, 313, 318] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Unavailable(function))
        );
        assert_eq!(
            evaluate(&game, NON_ACTOR, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(NON_ACTOR)))
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, 0, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}
