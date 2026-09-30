// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-65 behavior graph float conditions.

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
    order.add("Ai65.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_floats
        .insert("speedmult".into(), 1.25);
    game
}

fn condition(name: Option<&str>, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 447,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [name.map(str::to_owned), None],
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
        },
        c,
    )
}

#[test]
fn ai_65_graph_float_matches_and_rejects_a_different_value() {
    let game = fixture();
    assert!(conditions::supports_function(447));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(Some("SpeedMult"), 1.25)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(Some("SpeedMult"), 2.0)),
        Ok(false)
    );
}

#[test]
fn ai_65_graph_float_reports_missing_name_and_nonactor_context() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(None, 0.0)),
        Err(conditions::Error::Variable("missing CIS1".into()))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(Some("unknown"), 0.0)),
        Err(conditions::Error::Variable("unknown".into()))
    );
    assert_eq!(
        evaluate(&game, id(0xfff), &condition(Some("SpeedMult"), 0.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );
}
