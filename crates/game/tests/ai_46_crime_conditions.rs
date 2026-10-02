// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-46 crime-related CTDA predicates.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai46.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
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

/// ai_46_bribe_success_reads_the_shared_player_bribed_status
#[test]
fn ai_46_bribe_success_reads_the_shared_player_bribed_status() {
    let game = game();
    assert!(conditions::supports_function(654));

    assert_eq!(evaluate(&game, id(0x900), &condition(654, 1.0)), Ok(false));
    game.world_mut()
        .set_actor_value(id(0x900), "lastbribedintimidated", 1.0)
        .unwrap();
    assert_eq!(evaluate(&game, id(0x900), &condition(654, 1.0)), Ok(true));
}

/// ai_46_intimidate_success_requires_the_unavailable_perk_entry_point
#[test]
fn ai_46_intimidate_success_requires_the_unavailable_perk_entry_point() {
    let game = game();
    assert!(conditions::supports_function(655));

    for confidence in [0.0, 2.0, 4.0] {
        game.world_mut()
            .set_actor_value(id(0x900), "confidence", confidence)
            .unwrap();
        for expected in [0.0, 1.0] {
            assert_eq!(
                evaluate(&game, id(0x900), &condition(655, expected)),
                Err(conditions::Error::Unavailable(655))
            );
        }
    }
}

/// ai_46_arrested_state_reads_the_subject_arrest_flag
#[test]
fn ai_46_arrested_state_reads_the_subject_arrest_flag() {
    let game = game();
    assert!(conditions::supports_function(656));

    assert_eq!(evaluate(&game, id(0x900), &condition(656, 1.0)), Ok(false));
    game.world_mut().actor(id(0x900)).unwrap().arrested = true;
    assert_eq!(evaluate(&game, id(0x900), &condition(656, 1.0)), Ok(true));
}

/// ai_46_no_parameter_functions_reject_unused_parameter_slots
#[test]
fn ai_46_no_parameter_functions_reject_unused_parameter_slots() {
    let game = game();
    for function in [654, 655, 656] {
        let mut invalid = condition(function, 1.0);
        invalid.parameters[1] = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &invalid),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}

/// ai_46_actor_predicates_reject_a_subject_without_actor_context
#[test]
fn ai_46_actor_predicates_reject_a_subject_without_actor_context() {
    let game = game();
    for function in [654, 655, 656] {
        assert_eq!(
            evaluate(&game, id(0x901), &condition(function, 1.0)),
            Err(conditions::Error::Form(id(0x901)))
        );
    }
}

/// ai_46_arresting_actor_reports_the_missing_live_arrest_contract
#[test]
fn ai_46_arresting_actor_reports_the_missing_live_arrest_contract() {
    let game = game();
    assert!(!conditions::supports_function(657));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(657, 1.0)),
        Err(conditions::Error::Function(657))
    );
}
