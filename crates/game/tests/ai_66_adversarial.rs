// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-66 checks against xEdit's condition parameter enums.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai66Axis.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(0x900)).unwrap();
    game
}

fn condition(function: u16, axis: u32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(0.0),
        function,
        parameters: [axis, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, condition: &Condition) -> Result<bool, conditions::Error> {
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
        condition,
    )
}

#[test]
fn ai_66_pathing_offset_uses_xedit_axis_enum_values() {
    let game = game();

    // TES5Edit wbAxisEnum assigns X, Y and Z the values 88, 89 and 90.
    let observed = [88, 89, 90, 0].map(|axis| evaluate(&game, &condition(619, axis)));
    assert_eq!(
        observed,
        [
            Err(conditions::Error::Contract(619)),
            Err(conditions::Error::Contract(619)),
            Err(conditions::Error::Contract(619)),
            Err(conditions::Error::Parameter(619, 0)),
        ],
        "valid source axes must reach the missing runtime contract, while zero is invalid"
    );
}
