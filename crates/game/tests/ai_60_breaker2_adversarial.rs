// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Additional AI-60 casting-source checks against xEdit's enum contract.

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
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("AI60CastingSources.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, source: u32) -> Result<bool, conditions::Error> {
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 571,
        parameters: [source, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(0x801),
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
        &condition,
    )
}

#[test]
fn ai_60_current_casting_type_accepts_all_xedit_casting_sources() {
    let game = fixture();
    for source in 0..=3 {
        assert!(conditions::supports_function(571));
        assert_eq!(evaluate(&game, source), Ok(false), "source {source}");
    }
    assert_eq!(
        evaluate(&game, 4),
        Err(conditions::Error::Parameter(571, 4))
    );
}
