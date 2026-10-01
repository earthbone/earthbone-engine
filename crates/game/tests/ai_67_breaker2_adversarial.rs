// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai67Breaker2.esm", plugin).unwrap();
    Game::new(order)
}
fn eval(
    game: &Game,
    subject: GlobalFormId,
    function: u16,
    expected: f32,
) -> Result<bool, conditions::Error> {
    let c = Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
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
        &c,
    )
}

#[test]
fn ai_67_being_ridden_is_distinct_from_riding_a_mount() {
    let game = game();
    {
        let mut world = game.world_mut();
        world.actor(id(0x900)).unwrap().riding_mount = true;
    }
    assert_eq!(eval(&game, id(0x900), 714, 0.0), Ok(true));
    game.world_mut().actor(id(0x900)).unwrap().being_ridden = true;
    assert_eq!(eval(&game, id(0x900), 714, 1.0), Ok(true));
    assert_eq!(eval(&game, id(0x900), 714, 0.0), Ok(false));
    assert_eq!(
        eval(&game, id(0x910), 714, 1.0),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_current_path_speed_does_not_leak_after_arrival() {
    let game = game();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.path = vec![[4.0, 0.0, 0.0]];
        actor.ai.speed = 22.0;
        actor.ai.arrived = true;
    }
    assert_eq!(eval(&game, id(0x900), 683, 0.0), Ok(true));
    assert_eq!(eval(&game, id(0x900), 683, 22.0), Ok(false));
}
