// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-68 flying and flying-mount condition contracts.

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
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x830).sub(Tag(*b"MHDT"), [0_u8; 8]));
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x831));
    let mut cell_with_data = Record::new(Tag(*b"CELL"), 0x810).sub(Tag(*b"DATA"), [0_u8, 0]);
    cell_with_data.placement.world = Some(0x830);
    plugin.push_top(cell_with_data);
    let mut cell_without_data = Record::new(Tag(*b"CELL"), 0x811).sub(Tag(*b"DATA"), [0_u8, 0]);
    cell_without_data.placement.world = Some(0x831);
    plugin.push_top(cell_without_data);
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    let mut order = LoadOrder::new();
    order.add("FlyingConditions.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x810));
    game
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
            light_level: None,
            skill_use: None,
        },
        c,
    )
}

#[test]
fn ai_68_can_fly_here_uses_worldspace_max_height_data() {
    let game = fixture();
    let can_fly = condition(731, 1.0);
    assert!(conditions::supports_function(731));
    assert_eq!(evaluate(&game, id(0x900), &can_fly), Ok(true));

    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x811));
    assert_eq!(evaluate(&game, id(0x900), &can_fly), Ok(false));
    game.world_mut().reference(id(0x900)).unwrap().cell = Some(id(0x810));
    assert_eq!(evaluate(&game, id(0x900), &can_fly), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(731, 0.0)), Ok(false));
}

#[test]
fn ai_68_can_fly_here_rejects_parameters_and_invalid_actor_context() {
    let game = fixture();
    let mut parameter = condition(731, 0.0);
    parameter.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &parameter),
        Err(conditions::Error::Parameter(731, 1))
    );
    for subject in [id(0x820), id(0xfff)] {
        assert_eq!(
            evaluate(&game, subject, &condition(731, 0.0)),
            Err(conditions::Error::Form(subject))
        );
    }

    game.world_mut().reference(id(0x900)).unwrap().cell = None;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(731, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
    game.world_mut().reference(id(0x900)).unwrap().deleted = true;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(731, 0.0)),
        Err(conditions::Error::Form(id(0x900)))
    );
}

#[test]
fn ai_68_flying_mount_conditions_report_missing_contracts() {
    let game = fixture();
    for function in [732, 733] {
        assert!(!conditions::supports_function(function));
        for expected in [0.0, 1.0] {
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, expected)),
                Err(conditions::Error::Function(function))
            );
        }
        let mut missing_target = condition(function, 1.0);
        missing_target.run_on = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &missing_target),
            Err(conditions::Error::Target(1))
        );
    }
}
