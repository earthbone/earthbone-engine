// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai65Breaker2.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, params: [u32; 2], expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: params,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn eval(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
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

#[test]
fn ai_65_graph_float_is_case_insensitive_and_rejects_second_parameter() {
    let game = game();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_floats
        .insert("speedblend".into(), 1.5);
    let mut c = condition(447, [0, 0], 1.5);
    c.strings[0] = Some("SPEEDBLEND".into());
    assert_eq!(eval(&game, id(0x900), &c), Ok(true));
    c.parameters[1] = 1;
    assert_eq!(
        eval(&game, id(0x900), &c),
        Err(conditions::Error::Parameter(447, 1))
    );
}

#[test]
fn ai_65_unknown_motion_functions_are_diagnostic_without_actor_lookup() {
    let game = game();
    for function in [358, 450, 584, 589, 613] {
        assert_eq!(
            eval(&game, id(0x910), &condition(function, [0, 0], 1.0)),
            Err(conditions::Error::UnavailableContract(function))
        );
    }
}
