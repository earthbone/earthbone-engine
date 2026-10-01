// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-86 diagnostics for conditions without sourced numeric contracts.

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
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, function: u16) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(0x900),
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
        &condition(function),
    )
}

#[test]
fn ai_86_is_killer_object_has_no_sourced_numeric_contract() {
    let game = fixture();

    assert!(!conditions::supports_function(409));
    assert_eq!(evaluate(&game, 409), Err(conditions::Error::Function(409)));
}

#[test]
fn ai_86_get_xp_for_next_level_has_no_sourced_numeric_contract() {
    let game = fixture();

    assert!(!conditions::supports_function(530));
    assert_eq!(evaluate(&game, 530), Err(conditions::Error::Function(530)));
}
