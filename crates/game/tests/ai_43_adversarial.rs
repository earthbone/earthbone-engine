// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-43 crime condition coverage. The local CK pages define intimidation,
//! trespassing and the actor's crime faction. Other named conditions have
//! signatures in xEdit but no usable return contract in the CK corpus.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

const OWNER: u32 = 0x800;
const SPEAKER_BASE: u32 = 0x801;
const SPEAKER: u32 = 0x900;
const CRIME_FACTION: u32 = 0x701;
const OTHER_FACTION: u32 = 0x702;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"FACT"), CRIME_FACTION));
    plugin.push_top(Record::new(Tag(*b"FACT"), OTHER_FACTION));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), SPEAKER_BASE).sub(Tag(*b"CRIF"), CRIME_FACTION.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), SPEAKER).sub(Tag(*b"NAME"), SPEAKER_BASE.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ai43.esm", plugin).unwrap();
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
            skill_use: None,
            light_level: None,
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
fn ai_43_intimidation_matches_state_and_rejects_invalid_context() {
    let game = fixture();
    assert!(conditions::supports_function(116));
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(116, 0, 0.0)),
        Ok(true)
    );
    game.world_mut()
        .set_actor_value(id(SPEAKER), "lastbribedintimidated", 1.0)
        .unwrap();
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(116, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(116, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(116, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0xdead), &condition(116, 0, 1.0)),
        Err(conditions::Error::Form(id(0xdead)))
    );

    let mut invalid_parameter = condition(116, 0, 1.0);
    invalid_parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(SPEAKER), &invalid_parameter),
        Err(conditions::Error::Parameter(116, 1))
    );
}

#[test]
fn ai_43_trespassing_matches_saved_actor_state_and_rejects_invalid_context() {
    let game = fixture();
    assert!(conditions::supports_function(145));
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(145, 0, 0.0)),
        Ok(true)
    );
    game.world_mut().actor(id(SPEAKER)).unwrap().trespassing = true;
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(145, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(145, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(145, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0xdead), &condition(145, 0, 1.0)),
        Err(conditions::Error::Form(id(0xdead)))
    );

    let bytes = game.world_mut().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        evaluate(&restored, id(SPEAKER), &condition(145, 0, 1.0)),
        Ok(true)
    );

    let mut invalid_parameter = condition(145, 0, 1.0);
    invalid_parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(SPEAKER), &invalid_parameter),
        Err(conditions::Error::Parameter(145, 1))
    );
}

#[test]
fn ai_43_crime_faction_matches_actor_contract_and_rejects_invalid_forms() {
    let game = fixture();
    assert!(conditions::supports_function(152));
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(152, CRIME_FACTION, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(152, CRIME_FACTION, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(152, CRIME_FACTION, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(152, OTHER_FACTION, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0xdead), &condition(152, CRIME_FACTION, 1.0)),
        Err(conditions::Error::Form(id(0xdead)))
    );
    assert_eq!(
        evaluate(&game, id(SPEAKER), &condition(152, SPEAKER_BASE, 1.0)),
        Err(conditions::Error::Form(id(SPEAKER_BASE)))
    );
}

#[test]
fn ai_43_conditions_without_ck_return_contract_report_unsupported_function() {
    let game = fixture();
    for function in [122, 144, 176, 190, 192] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(SPEAKER), &condition(function, 0, 0.0)),
            Err(conditions::Error::Function(function)),
            "function {function} has no return contract in the local CK corpus"
        );

        let mut missing_run_on_target = condition(function, 0, 0.0);
        missing_run_on_target.run_on = 1;
        assert_eq!(
            evaluate(&game, id(SPEAKER), &missing_run_on_target),
            Err(conditions::Error::Target(1)),
            "function {function} must still diagnose invalid run-on context"
        );
    }
}

#[test]
fn intimidated_outcome_survives_save_load_and_repeated_queries() {
    let game = fixture();
    game.world_mut()
        .set_actor_value(id(SPEAKER), "lastbribedintimidated", 1.0)
        .unwrap();
    let bytes = game.world_mut().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    for _ in 0..3 {
        assert_eq!(
            evaluate(&restored, id(SPEAKER), &condition(116, 0, 1.0)),
            Ok(true)
        );
    }
}
