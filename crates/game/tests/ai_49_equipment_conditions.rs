// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-49 equipment CTDA conditions over form records and actor equipment.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn record(tag: [u8; 4], object: u32, editor_id: &str) -> Record {
    Record::new(Tag(tag), object).sub(Tag(*b"EDID"), [editor_id.as_bytes(), &[0]].concat())
}

fn armor(object: u32, editor_id: &str, value: i32, slots: u32, kind: u32) -> Record {
    record(*b"ARMO", object, editor_id)
        .sub(
            Tag(*b"DATA"),
            [value.to_le_bytes(), 1.0_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), 100_u32.to_le_bytes())
        .sub(
            Tag(*b"BOD2"),
            [slots.to_le_bytes(), kind.to_le_bytes()].concat(),
        )
}

fn weapon(object: u32, editor_id: &str, animation: u8, skill: i32) -> Record {
    let mut dnam = vec![0; 100];
    dnam[0] = animation;
    dnam[76..80].copy_from_slice(&skill.to_le_bytes());
    record(*b"WEAP", object, editor_id)
        .sub(
            Tag(*b"DATA"),
            [
                &0_u32.to_le_bytes()[..],
                &1.0_f32.to_le_bytes()[..],
                &10_u16.to_le_bytes()[..],
            ]
            .concat(),
        )
        .sub(Tag(*b"DNAM"), dnam)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record(*b"NPC_", 0x800, "ActorBase"));
    plugin.push_top(record(*b"STAT", 0x820, "StaticBase"));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x821).sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes()));
    plugin.push_top(record(*b"MISC", 0x810, "Gold001"));
    plugin.push_top(
        record(*b"ARMO", 0x811, "Clothes")
            .sub(
                Tag(*b"DATA"),
                [20_i32.to_le_bytes(), 1.0_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"DNAM"), 100_u32.to_le_bytes())
            .sub(
                Tag(*b"BOD2"),
                [1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(armor(0x812, "Armor", 80, 2, 0));
    plugin.push_top(armor(0x813, "Shield", 40, 0x200, 0));
    let mut torch_data = vec![0; 16];
    torch_data[12..16].copy_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(record(*b"LIGH", 0x814, "Torch").sub(Tag(*b"DATA"), torch_data));
    plugin.push_top(weapon(0x815, "Sword", 3, 8));
    plugin.push_top(record(*b"MISC", 0x816, "OtherItem"));
    let mut order = LoadOrder::new();
    order.add("AI49.esp", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.actor(id(0x801)).unwrap();
        world.add_item(id(0x801), id(0x810), 5);
        let actor = world.actor(id(0x801)).unwrap();
        actor
            .equipped
            .extend([id(0x811), id(0x812), id(0x813), id(0x814)]);
        actor.right_weapon = Some(id(0x815));
        actor.equipped.insert(id(0x815));
    }
    game
}

fn condition(function: u16, parameters: [u32; 2], expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters,
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_49_clothing_value_gold_and_equipment_state_match_records() {
    let game = fixture();
    for (function, value) in [
        (41, 20.0),
        (48, 5.0),
        (102, 1.0),
        (103, 1.0),
        (108, 3.0),
        (109, 1.0),
    ] {
        let parameter = if function == 109 { 8 } else { 0 };
        let positive = condition(function, [parameter, 0], value);
        assert!(conditions::supports_function(function));
        assert_eq!(
            eval(&game, id(0x801), &positive),
            Ok(true),
            "function {function}"
        );
        let negative = condition(function, [parameter, 0], value + 1.0);
        assert_eq!(
            eval(&game, id(0x801), &negative),
            Ok(false),
            "function {function}"
        );
    }

    let empty = fixture();
    {
        let mut world = empty.world_mut();
        let actor = world.actor(id(0x801)).unwrap();
        actor.equipped.clear();
        actor.left_weapon = None;
        actor.right_weapon = None;
        world.remove_item(id(0x801), id(0x810), 5, None);
    }
    for function in [41, 48, 102, 103, 108, 109] {
        let parameter = if function == 109 { 8 } else { 0 };
        let no_equipment = condition(function, [parameter, 0], 1.0);
        assert_eq!(
            eval(&empty, id(0x801), &no_equipment),
            Ok(false),
            "function {function} without equipment"
        );
    }
}

#[test]
fn ai_49_used_item_conditions_report_missing_action_context() {
    let game = fixture();
    for (function, parameter) in [(246, 0x816), (247, 32)] {
        let c = condition(function, [parameter, 0], 1.0);
        assert!(conditions::supports_function(function));
        assert_eq!(
            eval(&game, id(0x801), &c),
            Err(conditions::Error::UsedItemContext(function))
        );
        assert_eq!(
            eval(&game, id(0xfff), &c),
            Err(conditions::Error::Form(id(0xfff)))
        );
    }
}

#[test]
fn ai_49_equipment_conditions_reject_invalid_parameters_and_context() {
    let game = fixture();
    for function in [41, 48, 102, 103, 108] {
        let c = condition(function, [1, 0], 0.0);
        assert_eq!(
            eval(&game, id(0x801), &c),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    for function in [41, 102, 103, 108, 109] {
        let c = condition(function, [0, 0], 0.0);
        assert_eq!(
            eval(&game, id(0x821), &c),
            Err(conditions::Error::Form(id(0x821)))
        );
    }
    let missing_inventory = condition(48, [0, 0], 0.0);
    assert_eq!(
        eval(&game, id(0xfff), &missing_inventory),
        Err(conditions::Error::Form(id(0xfff)))
    );
    let invalid_skill = condition(109, [u32::MAX, 0], 1.0);
    assert_eq!(eval(&game, id(0x801), &invalid_skill), Ok(false));
}
