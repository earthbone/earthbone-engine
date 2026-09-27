// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-74: CTDA state queries from the named xEdit and CK references.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{ai, conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x801));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai74.esm", plugin).unwrap();
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
        },
        condition,
    )
}

#[test]
fn ai_74_misc_stat_reads_tracked_stat_and_rejects_unknown_enum_values() {
    let game = fixture();
    game.world_mut().stats.insert("Locks Picked".into(), 5);
    let positive = condition(312, 0x5829CC2E, 5.0);
    let negative = condition(312, 0x5829CC2E, 6.0);
    assert!(conditions::supports_function(312));
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &negative), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(312, u32::MAX, 0.0)),
        Err(conditions::Error::Parameter(312, u32::MAX))
    );
    let invalid_second_parameter = Condition {
        parameters: [0x5829CC2E, 1],
        ..positive
    };
    assert_eq!(
        evaluate(&game, id(0x900), &invalid_second_parameter),
        Err(conditions::Error::Parameter(312, 1))
    );
}

#[test]
fn ai_74_victim_query_uses_witnessed_crime_knowledge() {
    let game = fixture();
    game.world_mut()
        .actor(id(0x902))
        .unwrap()
        .witnessed_crimes
        .insert(id(0x900));
    let positive = condition(314, 0, 1.0);
    let negative = condition(314, 0, 1.0);
    assert!(conditions::supports_function(314));
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(evaluate(&game, id(0x901), &negative), Ok(false));
    let invalid = Condition {
        parameters: [1, 0],
        ..positive
    };
    assert_eq!(
        evaluate(&game, id(0x900), &invalid),
        Err(conditions::Error::Parameter(314, 1))
    );
}

#[test]
fn ai_74_flee_actor_and_essential_queries_match_live_actor_state() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().ai.procedure = ai::Procedure::Flee;
    game.world_mut().actor(id(0x901)).unwrap().essential = true;
    for function in [329, 353, 354] {
        assert!(conditions::supports_function(function));
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(329, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(329, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(353, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(353, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(354, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(354, 0, 1.0)),
        Ok(false)
    );
    for function in [329, 353, 354] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}

#[test]
fn ai_74_has_linked_ref_matches_keyword_filtered_links() {
    let game = fixture();
    game.world_mut().reference(id(0x900)).unwrap().linked = vec![(Some(id(0x801)), id(0x910))];
    assert!(conditions::supports_function(362));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(362, 0x801, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(362, 0x801, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(362, 0x800, 1.0)),
        Err(conditions::Error::Form(id(0x800)))
    );
    let invalid = Condition {
        parameters: [0x801, 1],
        ..condition(362, 0x801, 0.0)
    };
    assert_eq!(
        evaluate(&game, id(0x900), &invalid),
        Err(conditions::Error::Parameter(362, 1))
    );
}

#[test]
fn ai_74_player_action_queries_report_missing_contract_explicitly() {
    let game = fixture();
    for function in [367, 368] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 1.0)),
            Err(conditions::Error::Function(function))
        );
    }
}
