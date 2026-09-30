// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based adversarial checks for AI-85 condition functions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"WATR"), 0x812));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x912).sub(Tag(*b"NAME"), 0x812_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai85Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x910),
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
fn ai_85_is_water_object_distinguishes_water_from_non_water_references() {
    // UESP Raw_Function_Data.wikitext describes this as the water condition
    // used for drinking animation, so it must evaluate water and non-water refs.
    let game = fixture();
    assert!(conditions::supports_function(304));
    assert_eq!(evaluate(&game, id(0x912), &condition(304, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x910), &condition(304, 1.0)), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x812), &condition(304, 1.0)),
        Err(conditions::Error::Form(id(0x812)))
    );
    let mut parameter = condition(304, 1.0);
    parameter.parameters[0] = 1;
    assert_eq!(
        evaluate(&game, id(0x912), &parameter),
        Err(conditions::Error::Parameter(304, 1))
    );
}

#[test]
fn ai_85_get_restrained_uses_actor_flag_without_mutating_it_for_query() {
    // CK SetRestrained defines a persistent actor flag. A condition query must
    // read it without changing the flag or requiring an unrelated target.
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai85Restrained.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().actor(id(0x900)).unwrap().restrained = true;
    let query = condition(244, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
    assert!(game.world_mut().actor(id(0x900)).unwrap().restrained);
}
