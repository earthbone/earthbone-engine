// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-41 combat CTDA conditions from xEdit and the CK Actor reference.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, package, Game};

const BASE: u32 = 0x800;
const SUBJECT: u32 = 0x900;
const TARGET: u32 = 0x901;
const KEYWORD: u32 = 0x810;
const IGNORE_PACKAGE: u32 = 0x850;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801).sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x811));
    plugin.push_top(Record::new(Tag(*b"ACHR"), SUBJECT).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), TARGET).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut package_data = [0; 12];
    package_data[..4].copy_from_slice(&package::flags::IGNORE_COMBAT.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"PACK"), IGNORE_PACKAGE).sub(Tag(*b"PKDT"), package_data));
    let mut order = LoadOrder::new();
    order.add("AI41.esm", plugin).unwrap();
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

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(BASE),
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

#[test]
fn ai_41_combat_target_keyword_matches_and_rejects_nonmatching_keyword() {
    let game = fixture();
    game.world_mut().actor(id(SUBJECT)).unwrap().combat_target = Some(id(TARGET));

    assert!(conditions::supports_function(707));
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(707, KEYWORD, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(707, 0x811, 0, 1.0)),
        Ok(false)
    );
}

#[test]
fn ai_41_combat_target_keyword_checks_missing_target_and_invalid_context() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(707, KEYWORD, 0, 1.0)),
        Ok(false)
    );

    let bad_keyword = evaluate(&game, id(SUBJECT), &condition(707, 0xfff, 0, 1.0));
    assert_eq!(bad_keyword, Err(conditions::Error::Form(id(0xfff))));
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(707, KEYWORD, 0x811, 1.0)),
        Err(conditions::Error::Parameter(707, 0x811))
    );

    let mut missing_context = condition(707, KEYWORD, 0, 1.0);
    missing_context.run_on = 1;
    assert_eq!(
        evaluate(&game, id(SUBJECT), &missing_context),
        Err(conditions::Error::Target(1))
    );

    game.world_mut().actor(id(SUBJECT)).unwrap().combat_target = Some(id(0xfff));
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(707, KEYWORD, 0, 1.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );
}

#[test]
fn ai_41_ignoring_combat_reads_package_flag_and_validates_context() {
    let game = fixture();
    assert!(conditions::supports_function(710));
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(710, 0, 0, 1.0)),
        Ok(false)
    );
    game.world_mut().actor(id(SUBJECT)).unwrap().ai.package = Some(id(IGNORE_PACKAGE));
    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(710, 0, 0, 1.0)),
        Ok(true)
    );

    assert_eq!(
        evaluate(&game, id(SUBJECT), &condition(710, 1, 0, 1.0)),
        Err(conditions::Error::Parameter(710, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x810), &condition(710, 0, 0, 1.0)),
        Err(conditions::Error::Form(id(0x810)))
    );
}

#[test]
fn ai_41_conditions_without_usable_contract_return_named_diagnostics() {
    let game = fixture();
    for (function, reason) in [
        (702, "the runtime tracks stagger state, but no recoil state"),
        (705, "CK does not define the actor-to-actor help decision"),
        (709, "the runtime does not model combat group membership"),
        (
            719,
            "CK names hostility but does not define its faction and crime rules",
        ),
    ] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(SUBJECT), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::UnavailableContractReason(
                function, reason
            )),
            "function {function}"
        );
    }
}
