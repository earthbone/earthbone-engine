// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! CMB-04 press routing edge cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};
use soe_godot_host::combat::MeleeContact;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_weapon() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for (slot, parents) in [
        (0xb00_u32, None),
        (0xb01, None),
        (0xb02, Some((0xb00_u32, 0xb01_u32))),
    ] {
        let mut record = Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), 0_u32.to_le_bytes());
        if let Some((left, right)) = parents {
            let mut bytes = left.to_le_bytes().to_vec();
            bytes.extend_from_slice(&right.to_le_bytes());
            record = record.sub(Tag(*b"PNAM"), bytes);
        }
        plugin.push_top(record);
    }
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x31).sub(Tag(*b"DNAM"), defaults));
    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xa00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam)
            .sub(Tag(*b"ETYP"), 0xb02_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Press.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    game.world_mut().add_item(player, id(0xa00), 1);
    game.world_mut()
        .equip_hand_weapon(player, Hand::Right, id(0xa00))
        .unwrap();
    game
}

#[test]
fn held_frames_do_not_arm_a_swing_without_a_press_edge() {
    let mut game = game_with_weapon();
    let mut contact = MeleeContact::default();
    for _ in 0..3 {
        assert!(!contact.begin_from_attack_press(&mut game, false).unwrap());
        assert_eq!(contact.armed(), None);
    }
}

#[test]
fn missing_right_weapon_rejects_press_without_leaving_a_stale_swing() {
    let mut game = game_with_weapon();
    let player = game.player();
    game.world_mut().actor(player).unwrap().equipped.clear();
    let mut contact = MeleeContact::default();
    assert!(contact.begin_from_attack_press(&mut game, true).is_err());
    assert_eq!(contact.armed(), None);

    game.world_mut().add_item(player, id(0xa00), 1);
    game.world_mut()
        .equip_hand_weapon(player, Hand::Right, id(0xa00))
        .unwrap();
    assert!(contact.begin_from_attack_press(&mut game, true).unwrap());
    assert_eq!(contact.armed(), Some((player, Hand::Right)));
}
