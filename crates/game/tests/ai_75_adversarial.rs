// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

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
    order.add("AI75Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, function: u16) -> Result<bool, conditions::Error> {
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(0.0),
        function,
        parameters: [0, 0],
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
fn ai_75_group_member_count_reports_missing_ck_contract() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, 415),
        Err(conditions::Error::UnavailableContract(415))
    );
}

#[test]
fn ai_75_group_target_count_reports_missing_ck_contract() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, 416),
        Err(conditions::Error::UnavailableContract(416))
    );
}
