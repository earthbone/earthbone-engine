// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-38 combat CTDA functions and diagnostics for conditions without a local contract.

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
    order.add("Ai38.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    game
}

fn condition(function: u16, param1: u32, param2: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [param1, param2],
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
        },
        c,
    )
}

#[test]
fn ai_38_dead_count_tracks_dead_actors_with_the_same_base() {
    let game = fixture();
    let query = condition(84, 0x800, 0, 1.0);
    assert!(conditions::supports_function(84));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(84, 0x800, 0, 0.0)),
        Ok(true)
    );
    assert!(game.world_mut().kill_actor(id(0x900), None));
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(84, 0x800, 0, 2.0)),
        Ok(false)
    );
    game.world_mut().resurrect_actor(id(0x900)).unwrap();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(84, 0x800, 0, 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_38_combat_state_reports_unengaged_and_engaged_values() {
    let game = fixture();
    assert!(conditions::supports_function(323));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(323, 0, 0, 0.0)),
        Ok(true)
    );
    game.world_mut().actor(id(0x900)).unwrap().in_combat = true;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(323, 0, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(323, 0, 0, 0.0)),
        Ok(false)
    );
}

#[test]
fn ai_38_ignore_friendly_hits_matches_reference_state() {
    let game = fixture();
    assert!(conditions::supports_function(338));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(338, 0, 0, 0.0)),
        Ok(true)
    );
    game.world_mut()
        .reference(id(0x900))
        .unwrap()
        .ignore_friendly_hits = true;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(338, 0, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(338, 0, 0, 0.0)),
        Ok(false)
    );
}

#[test]
fn ai_38_documented_queries_reject_invalid_context_and_parameters() {
    let game = fixture();
    for function in [84, 323, 338] {
        if function != 84 {
            assert_eq!(
                evaluate(&game, id(0x800), &condition(function, 0, 0, 0.0)),
                Err(conditions::Error::Form(id(0x800))),
                "function {function} requires its CK object class"
            );
        }
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1)),
            "function {function} has no second parameter"
        );
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(84, 0x900, 0, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(84, 0x900, 0, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
}

#[test]
fn ai_38_undocumented_conditions_return_function_diagnostics() {
    let game = fixture();
    for function in [63, 66, 118, 288, 361] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Function(function)),
            "function {function} has no usable CK condition contract in the local corpus"
        );
    }
}
