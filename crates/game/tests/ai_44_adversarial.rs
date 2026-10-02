// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-44 adversarial persistence coverage for faction crime conditions.

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
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x810));
    let mut order = LoadOrder::new();
    order.add("CrimeAdversarial.esm", plugin).unwrap();
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
    function: u16,
    parameter: u32,
    expected: f32,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject: id(0x900),
            target: None,
            quest: None,
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        &condition(function, parameter, expected),
    )
}

#[test]
fn faction_crime_gold_values_survive_save_load_and_keep_ledger_split() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let faction = world.faction(id(0x810)).unwrap();
        faction.crime_gold_violent = 17;
        faction.crime_gold_nonviolent = 29;
    }
    let bytes = game.world_mut().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(evaluate(&restored, 375, 0x810, 17.0), Ok(true));
    assert_eq!(evaluate(&restored, 375, 0x810, 29.0), Ok(false));
    assert_eq!(evaluate(&restored, 376, 0x810, 29.0), Ok(true));
    assert_eq!(evaluate(&restored, 376, 0x810, 17.0), Ok(false));
}
