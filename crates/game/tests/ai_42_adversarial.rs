// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-42 SKSE compatibility conditions and their unavailable context.

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
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
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

fn evaluate(game: &Game, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(0x910),
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
fn ai_42_version_queries_match_absent_skse_and_reject_nonzero_checks() {
    let game = fixture();
    for function in [1024, 1025, 1026, 1027] {
        assert!(conditions::supports_function(function), "{function}");
        assert_eq!(
            evaluate(&game, &condition(function, 0, 0.0)),
            Ok(true),
            "{function}"
        );
        assert_eq!(
            evaluate(&game, &condition(function, 0, 1.0)),
            Ok(false),
            "{function}"
        );
        assert_eq!(
            evaluate(&game, &condition(function, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1)),
            "{function} accepts no integer parameter"
        );
    }
}

#[test]
fn ai_42_compatibility_queries_report_missing_run_on_context() {
    let game = fixture();
    for function in [1024, 1025, 1026, 1027, 1028] {
        let mut c = condition(function, 0, 0.0);
        c.run_on = 1;
        assert_eq!(
            evaluate(&game, &c),
            Err(conditions::Error::Target(1)),
            "{function} must reject a missing target before evaluation"
        );
    }
}

#[test]
fn ai_42_clear_invalid_registrations_reports_no_query_contract() {
    let game = fixture();
    let c = condition(1028, 0, 0.0);
    assert!(conditions::supports_function(1028));
    assert_eq!(
        evaluate(&game, &c),
        Err(conditions::Error::NoConditionContract(1028))
    );
    assert_eq!(
        evaluate(&game, &condition(1028, 1, 0.0)),
        Err(conditions::Error::Parameter(1028, 1))
    );
}
