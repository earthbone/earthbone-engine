// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-72 reference state conditions.

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
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(Record::new(Tag(*b"DOOR"), 0x801));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x810).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("StateAdversarial.esm", plugin).unwrap();
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

#[test]
fn get_open_state_distinguishes_open_and_closed_states() {
    let game = fixture();
    let door = id(0x810);
    // This runtime stores instantaneous open and closed states, not animation progress.
    game.world_mut().reference(door).unwrap().opened = true;
    assert_eq!(evaluate(&game, door, &condition(157, 0, 1.0)), Ok(true));
    game.world_mut().reference(door).unwrap().opened = false;
    assert_eq!(evaluate(&game, door, &condition(157, 0, 3.0)), Ok(true));
}

#[test]
fn ai_72_get_open_state_reports_opening_and_closing_transitions() {
    let game = fixture();
    let door = id(0x810);

    assert!(game.world_mut().begin_door_opening(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 2.0)), Ok(true));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 1.0)), Ok(false));
    assert!(game.world_mut().finish_door_transition(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 1.0)), Ok(true));

    assert!(game.world_mut().begin_door_closing(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 4.0)), Ok(true));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 3.0)), Ok(false));
    assert!(game.world_mut().finish_door_transition(door));
    assert_eq!(evaluate(&game, door, &condition(157, 0, 3.0)), Ok(true));

    assert!(!game.world_mut().begin_door_opening(id(0x800)));
}

#[test]
fn default_open_and_destroyed_reject_a_deleted_reference() {
    let game = fixture();
    let reference = id(0x810);
    game.world_mut().reference(reference).unwrap().deleted = true;
    for function in [157, 203, 215] {
        assert!(
            matches!(
                evaluate(&game, reference, &condition(function, 0, 0.0)),
                Err(conditions::Error::Form(_))
            ),
            "{function} accepted a deleted reference"
        );
    }
}
