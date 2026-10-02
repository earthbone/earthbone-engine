// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-44 crime CTDA functions with faction and actor state.

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
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x811));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    let mut order = LoadOrder::new();
    order.add("CrimeConditions.esm", plugin).unwrap();
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
            skill_use: None,
            light_level: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        condition,
    )
}

#[test]
fn ai_44_player_expelled_reads_the_named_faction_flag() {
    let game = fixture();
    assert!(conditions::supports_function(193));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(193, 0x810, 1.0)),
        Ok(false)
    );

    game.world_mut().faction(id(0x810)).unwrap().player_expelled = true;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(193, 0x810, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(193, 0x810, 0.0)),
        Ok(false)
    );
}

#[test]
fn ai_44_crime_gold_conditions_read_each_faction_ledger() {
    let game = fixture();
    for (function, expected) in [(375, 125.0), (376, 45.0)] {
        assert!(conditions::supports_function(function));
        let mut world = game.world_mut();
        let faction = world.faction(id(0x810)).unwrap();
        faction.crime_gold_violent = 125;
        faction.crime_gold_nonviolent = 45;
        drop(world);
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0x810, expected)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                id(0x900),
                &condition(function, 0x810, expected + 1.0)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_44_crime_gold_null_parameter_uses_subject_crime_faction() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().crime_faction = Some(id(0x810));
    game.world_mut()
        .faction(id(0x810))
        .unwrap()
        .crime_gold_violent = 90;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(375, 0, 90.0)),
        Ok(true)
    );

    game.world_mut().actor(id(0x900)).unwrap().crime_faction = None;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(375, 0, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
}

#[test]
fn ai_44_bribed_condition_reads_the_persistent_actor_outcome_value() {
    let game = fixture();
    assert!(conditions::supports_function(402));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(402, 0, 1.0)),
        Ok(false)
    );

    game.world_mut()
        .set_actor_value(id(0x900), "lastbribedintimidated", 1.0)
        .unwrap();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(402, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(402, 0, 0.0)),
        Ok(false)
    );
}

#[test]
fn ai_44_faction_conditions_reject_wrong_forms_and_unused_parameters() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(193, 0x820, 0.0)),
        Err(conditions::Error::Form(id(0x820)))
    );
    let mut extra = condition(193, 0x810, 0.0);
    extra.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &extra),
        Err(conditions::Error::Parameter(193, 1))
    );
    let mut extra = condition(402, 0, 0.0);
    extra.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &extra),
        Err(conditions::Error::Parameter(402, 1))
    );
}

#[test]
fn ai_44_functions_without_a_usable_history_contract_report_it() {
    let game = fixture();
    for function in [195, 282, 366, 373] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0.0)),
            Err(conditions::Error::Contract(function)),
            "function {function}"
        );
    }
}

#[test]
fn ai_44_all_crime_functions_reject_missing_run_on_context() {
    let game = fixture();
    for function in [193, 195, 282, 366, 373, 375, 376, 402] {
        let mut condition = condition(function, 0, 0.0);
        condition.run_on = 3;
        assert_eq!(
            evaluate(&game, id(0x900), &condition),
            Err(conditions::Error::Target(3)),
            "function {function}"
        );
    }
}
