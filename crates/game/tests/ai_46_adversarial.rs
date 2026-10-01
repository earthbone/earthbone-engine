// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial AI-46 crime condition cases.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai46Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, expected: f32, parameters: [u32; 2]) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    condition: &Condition,
) -> Result<bool, conditions::Error> {
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
        condition,
    )
}

#[test]
fn ai_46_bribe_success_false_comparison_tracks_only_subject_status() {
    let game = game();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(654, 0.0, [0, 0])),
        Ok(true)
    );
    game.world_mut()
        .set_actor_value(id(0x900), "lastbribedintimidated", 1.0)
        .unwrap();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(654, 0.0, [0, 0])),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(654, 1.0, [0, 0])),
        Ok(false)
    );
}

#[test]
fn ai_46_intimidate_success_requires_the_authored_perk_formula() {
    let game = game();
    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 60.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x900), "confidence", 2.0)
        .unwrap();

    // CK documents WillIntimidateSucceed as using the Mod Player Intimidation
    // perk entry point. This fixture has no evaluated entry point context.
    assert_eq!(
        evaluate(&game, id(0x900), &condition(655, 1.0, [0, 0])),
        Err(conditions::Error::Unavailable(655))
    );
}

#[test]
fn ai_46_intimidate_success_does_not_treat_missing_confidence_as_a_contract() {
    let game = game();
    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 100.0)
        .unwrap();

    assert_eq!(
        evaluate(&game, id(0x901), &condition(655, 1.0, [0, 0])),
        Err(conditions::Error::StateContract(655))
    );
}

#[test]
fn ai_46_all_implemented_functions_reject_both_parameter_slots() {
    let game = game();
    for function in [654, 655, 656] {
        for parameters in [[1, 0], [0, 1]] {
            let bad_slot = if parameters[0] != 0 {
                parameters[0]
            } else {
                parameters[1]
            };
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, 1.0, parameters)),
                Err(conditions::Error::Parameter(function, bad_slot))
            );
        }
    }
}

#[test]
fn ai_46_arrested_state_is_actor_local_and_tracks_clear_state() {
    let game = game();
    game.world_mut().actor(id(0x900)).unwrap().arrested = true;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(656, 1.0, [0, 0])),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(656, 1.0, [0, 0])),
        Ok(false)
    );
    game.world_mut().actor(id(0x900)).unwrap().arrested = false;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(656, 1.0, [0, 0])),
        Ok(false)
    );
}

#[test]
fn ai_46_arresting_actor_returns_a_contract_diagnostic_even_without_actor_context() {
    let game = game();
    assert_eq!(
        evaluate(&game, id(0x999), &condition(657, 1.0, [0, 0])),
        Err(conditions::Error::Function(657))
    );
}
