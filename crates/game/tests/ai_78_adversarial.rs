// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-78 reference functions.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("HeightAdversarial.esm", plugin).unwrap();
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
fn target_height_returns_the_vertical_delta_for_both_reference_orders() {
    let game = fixture();
    game.world_mut().reference(id(0x900)).unwrap().position = [0.0, 0.0, 4.0];
    game.world_mut().reference(id(0x901)).unwrap().position = [0.0, 0.0, 11.0];
    // UESP Raw_Function_Data defines this as the delta on Z between references.
    assert_eq!(
        evaluate(&game, id(0x900), &condition(720, 0x901, 7.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(720, 0x900, -7.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(720, 0x901, -7.0)),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(720, 0xffff, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0xffff), &condition(720, 0x901, 0.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn unknown_knock_and_activator_state_return_the_declared_contract_diagnostic() {
    let game = fixture();
    for function in [680, 725] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0.0)),
            Err(conditions::Error::Contract(function))
        );
    }
}
