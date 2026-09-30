// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-62 placement, sleep and unavailable motion condition contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{ai::Procedure, conditions, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const REFERENCE: u32 = 0x802;
const NON_REFERENCE: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn point(values: [f32; 3]) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [point([1.0, 2.0, 3.0]), point([0.0, 0.0, 0.0])].concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [point([10.0, 20.0, 30.0]), point([0.0, 0.0, 1.0])].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"BOOK"), NON_REFERENCE));
    let mut order = LoadOrder::new();
    order.add("Ai62.esm", plugin).unwrap();
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_62_position_and_angle_conditions_use_runtime_axes_and_degrees() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let reference = world.reference(id(REFERENCE)).unwrap();
        reference.position = [11.0, 22.0, 33.0];
        reference.rotation = [0.0, 0.0, std::f32::consts::FRAC_PI_2];
    }
    for (function, axis, value) in [(6, 1, 22.0), (8, 2, 90.0)] {
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, axis, 0, value)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, axis, 0, value + 1.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, NON_REFERENCE, &condition(function, axis, 0, value)),
            Err(conditions::Error::Form(id(NON_REFERENCE)))
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, 3, 0, value)),
            Err(conditions::Error::Parameter(function, 3))
        );
    }
}

#[test]
fn ai_62_starting_position_and_angle_conditions_use_authored_transform() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let reference = world.reference(id(REFERENCE)).unwrap();
        reference.position = [100.0, 200.0, 300.0];
        reference.rotation = [0.5, 0.5, 0.5];
    }
    for (function, axis, value) in [(10, 1, 20.0), (11, 2, 57.29578)] {
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, axis, 0, value)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, axis, 0, value + 1.0)),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, NON_REFERENCE, &condition(function, axis, 0, value)),
            Err(conditions::Error::Form(id(NON_REFERENCE)))
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, axis, 1, value)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}

#[test]
fn ai_62_sleeping_condition_reports_supported_actor_sleep_states() {
    let game = fixture();
    assert_eq!(evaluate(&game, ACTOR, &condition(49, 0, 0, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, ACTOR, &condition(49, 0, 0, 3.0)), Ok(false));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure = Procedure::Sleep;
    assert_eq!(evaluate(&game, ACTOR, &condition(49, 0, 0, 2.0)), Ok(true));
    game.world_mut().actor(id(ACTOR)).unwrap().ai.sleeping = true;
    assert_eq!(evaluate(&game, ACTOR, &condition(49, 0, 0, 3.0)), Ok(true));
    assert_eq!(
        evaluate(&game, REFERENCE, &condition(49, 0, 0, 3.0)),
        Err(conditions::Error::Form(id(REFERENCE)))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(49, 0, 1, 3.0)),
        Err(conditions::Error::Parameter(49, 1))
    );
}

#[test]
fn ai_62_motion_flags_report_unavailable_runtime_tracking() {
    let game = fixture();
    for function in [25, 26, 81] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, 0, 0, 1.0)),
            Err(conditions::Error::Unavailable(function))
        );
        assert_eq!(
            evaluate(&game, NON_REFERENCE, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(NON_REFERENCE)))
        );
        assert_eq!(
            evaluate(&game, REFERENCE, &condition(function, 0, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}
