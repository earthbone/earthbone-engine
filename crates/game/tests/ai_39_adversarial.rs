// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-39 combat condition contract coverage.

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
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
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
        },
        c,
    )
}

#[test]
fn ai_39_is_combat_target_matches_the_subject_target_relationship() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().combat_target = Some(id(0x901));

    let positive = condition(513, 0x901, 1.0);
    assert!(conditions::supports_function(513));
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(513, 0x902, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(513, 0x902, 1.0)),
        Ok(false)
    );
}

#[test]
fn ai_39_is_combat_target_rejects_invalid_actor_context() {
    let game = fixture();
    for parameter in [0, 0xfff, 0x910] {
        let c = condition(513, parameter, 1.0);
        assert!(matches!(
            evaluate(&game, id(0x900), &c),
            Err(conditions::Error::Form(_))
        ));
    }

    assert_eq!(
        evaluate(&game, id(0x910), &condition(513, 0x901, 1.0)),
        Err(conditions::Error::Form(id(0x910)))
    );
    let deleted = condition(513, 0x901, 1.0);
    game.world_mut().reference(id(0x901)).unwrap().deleted = true;
    assert_eq!(
        evaluate(&game, id(0x900), &deleted),
        Err(conditions::Error::Form(id(0x901)))
    );
}

#[test]
fn ai_39_undefined_combat_condition_contracts_return_explicit_diagnostics() {
    let game = fixture();
    for function in [396, 397, 477, 488, 495, 508, 574] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0.0)),
            Err(conditions::Error::Function(function)),
            "function {function}"
        );
    }
}
