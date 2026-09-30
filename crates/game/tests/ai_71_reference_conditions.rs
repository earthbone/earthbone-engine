// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-71 reference state condition contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{ai::Procedure, conditions, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const OTHER_ACTOR: u32 = 0x802;
const NOT_ACTOR: u32 = 0x803;
const GUARD_FACTION: u32 = 0x86eee;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(
        Tag(*b"SNAM"),
        [GUARD_FACTION.to_le_bytes().as_slice(), &[0]].concat(),
    ));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x805));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), OTHER_ACTOR).sub(Tag(*b"NAME"), 0x805_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"BOOK"), NOT_ACTOR));
    plugin.push_top(Record::new(Tag(*b"FACT"), GUARD_FACTION));
    let mut order = LoadOrder::new();
    order.add("Ai71.esm", plugin).unwrap();
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

#[test]
fn ai_71_alerted_condition_reads_actor_alert_state() {
    let game = fixture();
    game.world_mut().actor(id(ACTOR)).unwrap().alerted = true;
    assert!(conditions::supports_function(91));
    assert_eq!(evaluate(&game, ACTOR, &condition(91, 0, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, OTHER_ACTOR, &condition(91, 0, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, NOT_ACTOR, &condition(91, 0, 0, 0.0)),
        Err(conditions::Error::Form(id(NOT_ACTOR)))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(91, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(91, 1))
    );
}

#[test]
fn ai_71_waiting_condition_reads_wait_procedure() {
    let game = fixture();
    game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure = Procedure::Wait {
        stop_movement: false,
    };
    assert!(conditions::supports_function(111));
    assert_eq!(evaluate(&game, ACTOR, &condition(111, 0, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, OTHER_ACTOR, &condition(111, 0, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, NOT_ACTOR, &condition(111, 0, 0, 0.0)),
        Err(conditions::Error::Form(id(NOT_ACTOR)))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(111, 0, 1, 0.0)),
        Err(conditions::Error::Parameter(111, 1))
    );
}

#[test]
fn ai_71_guard_condition_reads_guard_faction_membership() {
    let game = fixture();
    assert!(conditions::supports_function(125));
    assert_eq!(evaluate(&game, ACTOR, &condition(125, 0, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, OTHER_ACTOR, &condition(125, 0, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, NOT_ACTOR, &condition(125, 0, 0, 0.0)),
        Err(conditions::Error::Form(id(NOT_ACTOR)))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(125, 0, 1, 0.0)),
        Err(conditions::Error::Parameter(125, 1))
    );
}

#[test]
fn ai_71_untracked_reference_queries_return_diagnostics() {
    let game = fixture();
    for function in [98, 106, 107, 110, 127] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 1.0)),
            Err(conditions::Error::Unavailable(function))
        );
        assert_eq!(
            evaluate(&game, OTHER_ACTOR, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Unavailable(function))
        );
        assert_eq!(
            evaluate(&game, NOT_ACTOR, &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(NOT_ACTOR)))
        );
    }
}
