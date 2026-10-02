// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Adversarial checks for the CMB-04 right attack press latch.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Error, Hand},
    Game,
};
use soe_godot_host::combat::MeleeContact;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for (slot, all) in [
        (0xb00, false),
        (0xb01, false),
        (0xb02, false),
        (0xb03, true),
    ] {
        let mut record =
            Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), u32::from(all).to_le_bytes());
        if slot == 0xb02 {
            let mut parents = 0xb00_u32.to_le_bytes().to_vec();
            parents.extend_from_slice(&0xb01_u32.to_le_bytes());
            record = record.sub(Tag(*b"PNAM"), parents);
        }
        plugin.push_top(record);
    }
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x31).sub(Tag(*b"DNAM"), defaults));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for object in [0xa00, 0xa01] {
        let mut data = vec![0; 10];
        data[8..10].copy_from_slice(&10_u16.to_le_bytes());
        let mut dnam = vec![0; 100];
        dnam[0] = 1;
        dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
        dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
        dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
        dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"WEAP"), object)
                .sub(Tag(*b"DATA"), data)
                .sub(Tag(*b"DNAM"), dnam)
                .sub(Tag(*b"ETYP"), 0xb02_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    order.add("AttackInput.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        for actor in [player, id(0x900), id(0x901)] {
            world.set_actor_value(actor, "health", 100.0).unwrap();
        }
        for weapon in [id(0xa00), id(0xa01)] {
            let count = if weapon == id(0xa01) { 2 } else { 1 };
            world.add_item(player, weapon, count);
        }
        world
            .equip_hand_weapon(player, Hand::Right, id(0xa00))
            .unwrap();
        world
            .equip_hand_weapon(player, Hand::Left, id(0xa01))
            .unwrap();
    }
    game
}

#[test]
fn held_frames_do_not_arm_until_a_new_right_attack_press_edge() {
    let mut game = fixture();
    let mut contact = MeleeContact::default();

    assert!(!contact.begin_from_attack_press(&mut game, false).unwrap());
    assert_eq!(contact.armed(), None);
    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());
    assert_eq!(contact.armed(), Some((id(0x14), Hand::Right)));
}

#[test]
fn repeated_press_cannot_replace_the_weapon_captured_by_the_armed_swing() {
    let mut game = fixture();
    let mut contact = MeleeContact::default();
    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());

    game.world_mut()
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa01))
        .unwrap();
    assert!(matches!(
        contact.begin_from_attack_press(&mut game, true),
        Err(Error::Unsupported("swing already awaiting contact"))
    ));
    assert_eq!(contact.armed(), Some((id(0x14), Hand::Right)));
    assert!(matches!(
        contact.event(&mut game, "HitFrame", |_| Ok(Some(id(0x900)))),
        Err(Error::Equipment)
    ));
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 100.0);
}

#[test]
fn failed_press_without_right_hand_weapon_does_not_arm_or_use_left_hand() {
    let mut game = fixture();
    game.world_mut()
        .actor(id(0x14))
        .unwrap()
        .equipped
        .remove(&id(0xa00));
    let mut contact = MeleeContact::default();

    assert!(matches!(
        contact.begin_from_attack_press(&mut game, true),
        Err(Error::Equipment)
    ));
    assert_eq!(contact.armed(), None);
    assert!(!contact.begin_from_attack_press(&mut game, false).unwrap());
    game.world_mut()
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa00))
        .unwrap();
    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());
}

#[test]
fn a_miss_consumes_one_contact_and_the_next_press_can_hit_once() {
    let mut game = fixture();
    let mut contact = MeleeContact::default();

    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());
    assert!(contact
        .event(&mut game, "HitFrame", |_| Ok(None))
        .unwrap()
        .is_none());
    assert_eq!(contact.armed(), None);
    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());
    assert!(contact
        .event(&mut game, "HitFrame", |_| Ok(Some(id(0x900))))
        .unwrap()
        .is_some());
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 90.0);
    assert!(contact
        .event(&mut game, "HitFrame", |_| {
            Err(Error::Unsupported("duplicate hit queried"))
        })
        .unwrap()
        .is_none());
}
