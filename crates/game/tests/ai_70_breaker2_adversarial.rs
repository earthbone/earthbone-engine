// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial AI-70 condition checks for missing sight observations.

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
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x911).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("AI70Breaker2.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().player();
    game
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
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

#[test]
fn ai_70_missing_sight_observation_is_false_and_comparison_can_invert_it() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(27, 0x901, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(27, 0x901, 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_70_sight_condition_rejects_invalid_reference_and_non_actor_subjects() {
    let game = fixture();
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(27, 0xdead, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x911), &condition(27, 0x900, 1.0)),
        Err(conditions::Error::Form(found)) if found == id(0x911)
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(27, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
}
