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
fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x830).sub(Tag(*b"MHDT"), [0u8; 8]));
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x831));
    let mut cell_with_data = Record::new(Tag(*b"CELL"), 0x810).sub(Tag(*b"DATA"), [0u8, 0]);
    cell_with_data.placement.world = Some(0x830);
    plugin.push_top(cell_with_data);
    let mut cell_without_data = Record::new(Tag(*b"CELL"), 0x811).sub(Tag(*b"DATA"), [0u8, 0]);
    cell_without_data.placement.world = Some(0x831);
    plugin.push_top(cell_without_data);
    let mut order = LoadOrder::new();
    order.add("AI68Adversarial.esp", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x810));
    game
}
fn c(function: u16, expected: f32) -> Condition {
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
fn eval(
    game: &Game,
    subject: GlobalFormId,
    condition: &Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
        },
        condition,
    )
}

#[test]
fn ai_68_can_fly_here_uses_worldspace_max_height_capability() {
    // CommonLib Actor::CanFlyHere checks TESWorldSpace::HasMaxHeightData.
    let game = fixture();
    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x811));
    assert_eq!(eval(&game, id(0x900), &c(731, 1.0)), Ok(false));
    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x810));
    assert_eq!(eval(&game, id(0x900), &c(731, 1.0)), Ok(true));
    assert_eq!(eval(&game, id(0x900), &c(731, 0.0)), Ok(false));
}

#[test]
fn ai_68_can_fly_here_requires_actor_and_cell_context() {
    let game = fixture();
    assert_eq!(
        eval(&game, id(0x800), &c(731, 0.0)),
        Err(conditions::Error::Form(id(0x800)))
    );
    game.world_mut().reference(id(0x900)).unwrap().cell = None;
    assert_eq!(
        eval(&game, id(0x900), &c(731, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
    let mut malformed = c(731, 0.0);
    malformed.parameters[0] = 1;
    assert_eq!(
        eval(&game, id(0x900), &malformed),
        Err(conditions::Error::Parameter(731, 1))
    );
}

#[test]
fn ai_68_flying_mount_route_states_remain_explicitly_unsupported() {
    let game = fixture();
    for function in [732, 733] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            eval(&game, id(0x900), &c(function, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}
