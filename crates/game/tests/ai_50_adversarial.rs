// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-50 equipment and inventory condition contracts from xEdit and CK pages.

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
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x901).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x820));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x821).sub(Tag(*b"KWDA"), 0x820_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0x830).sub(Tag(*b"DNAM"), weapon_dnam(1)));
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0x831).sub(Tag(*b"DNAM"), weapon_dnam(7)));
    plugin.push_top(Record::new(Tag(*b"FLST"), 0x840).sub(Tag(*b"LNAM"), 0x830_u32.to_le_bytes()));
    let mut light_data = vec![0; 16];
    light_data[12..16].copy_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 0x850).sub(Tag(*b"DATA"), light_data));
    let mut order = LoadOrder::new();
    order.add("AI50.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.actor(id(0x900)).unwrap();
        world.add_item(id(0x900), id(0x821), 3);
        world.add_item(id(0x901), id(0x821), 1);
        let actor = world.actor(id(0x900)).unwrap();
        actor.weapon_drawn = true;
        actor.left_weapon = Some(id(0x830));
        actor.right_weapon = Some(id(0x831));
        actor.equipped.insert(id(0x830));
        actor.equipped.insert(id(0x831));
        actor.equipped.insert(id(0x850));
    }
    game
}

fn weapon_dnam(animation: u8) -> Vec<u8> {
    let mut data = vec![0; 100];
    data[0] = animation;
    data
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

fn evaluate(subject: GlobalFormId, c: &Condition, game: &Game) -> Result<bool, conditions::Error> {
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
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        c,
    )
}

#[test]
fn ai_50_weapon_out_and_torch_conditions_read_equipped_actor_state() {
    let game = fixture();
    for (function, parameter) in [(263, 0), (306, 0)] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(id(0x900), &condition(function, parameter, 1.0), &game),
            Ok(true)
        );
        assert_eq!(
            evaluate(id(0x900), &condition(function, parameter, 0.0), &game),
            Ok(false)
        );
    }
    assert_eq!(
        evaluate(id(0x901), &condition(263, 0, 0.0), &game),
        Err(conditions::Error::Form(id(0x901)))
    );
    assert_eq!(
        evaluate(id(0x901), &condition(306, 0, 0.0), &game),
        Err(conditions::Error::Form(id(0x901)))
    );
}

#[test]
fn ai_50_weapon_list_uses_both_assigned_hands_and_validates_the_list() {
    let game = fixture();
    assert_eq!(
        evaluate(id(0x900), &condition(398, 0x840, 1.0), &game),
        Ok(true)
    );
    assert_eq!(
        evaluate(id(0x900), &condition(398, 0x840, 0.0), &game),
        Ok(false)
    );
    assert!(matches!(
        evaluate(id(0x900), &condition(398, 0x820, 1.0), &game),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_50_keyword_count_reads_inventory_and_checks_keyword_context() {
    let game = fixture();
    assert_eq!(
        evaluate(id(0x900), &condition(465, 0x820, 3.0), &game),
        Ok(true)
    );
    assert_eq!(
        evaluate(id(0x900), &condition(465, 0x820, 2.0), &game),
        Ok(false)
    );
    assert!(matches!(
        evaluate(id(0x900), &condition(465, 0x821, 0.0), &game),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_50_equipped_item_type_matches_ck_hand_values() {
    let game = fixture();
    for (hand, item_type) in [(0, 1), (1, 7)] {
        assert_eq!(
            evaluate(id(0x900), &condition(597, hand, item_type as f32), &game),
            Ok(true)
        );
        assert_eq!(
            evaluate(id(0x900), &condition(597, hand, 0.0), &game),
            Ok(false)
        );
    }
    assert_eq!(
        evaluate(id(0x900), &condition(597, 2, 0.0), &game),
        Err(conditions::Error::Parameter(597, 2))
    );
}

#[test]
fn ai_50_container_condition_matches_item_presence_and_rejects_bad_context() {
    let game = fixture();
    assert_eq!(
        evaluate(id(0x821), &condition(624, 0x901, 1.0), &game),
        Ok(true)
    );
    assert_eq!(
        evaluate(id(0x830), &condition(624, 0x901, 0.0), &game),
        Ok(true)
    );
    assert!(matches!(
        evaluate(id(0x821), &condition(624, 0x820, 1.0), &game),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_50_undocumented_equipment_conditions_report_contract_gaps() {
    let game = fixture();
    for function in [479, 487] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(id(0x900), &condition(function, 0, 1.0), &game),
            Err(conditions::Error::Contract(function))
        );
        assert_eq!(
            evaluate(id(0x901), &condition(function, 0, 1.0), &game),
            Err(conditions::Error::Contract(function))
        );
    }
}
