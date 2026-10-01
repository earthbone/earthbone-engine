// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-76 PlayerKnows accepts the knowable form types declared by xEdit.

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
    for (tag, object) in [
        (*b"WOOP", 0xA00),
        (*b"MGEF", 0xA01),
        (*b"ENCH", 0xA02),
        (*b"SPEL", 0xA03),
        (*b"SHOU", 0xA04),
    ] {
        plugin.push_top(Record::new(Tag(tag), object));
    }
    let mut order = LoadOrder::new();
    order.add("Ai76KnowableTypes.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, form: u32) -> Result<bool, conditions::Error> {
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 493,
        parameters: [form, 0],
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
        &condition,
    )
}

#[test]
fn ai_76_player_knows_accepts_words_of_power() {
    let game = fixture();
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .known_forms
        .insert(id(0xA00));

    assert_eq!(evaluate(&game, 0xA00), Ok(true));
}

#[test]
fn ai_76_player_knows_rejects_forms_outside_xedit_knowable_types() {
    let game = fixture();
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .spells
        .insert(id(0xA03));
    assert_eq!(evaluate(&game, 0xA03), Ok(true));
    assert_eq!(
        evaluate(&game, 0xA04),
        Err(conditions::Error::StateContract(493))
    );
}
