// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! INV-09 a shield and a two-handed weapon or bow cannot be equipped together.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};

const SHIELD: u32 = 0x800;
const SWORD: u32 = 0x801;
const GREATSWORD: u32 = 0x802;
const TORCH: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn weapon(object: u32, animation: u8) -> Record {
    let mut dnam = vec![0; 100];
    dnam[0] = animation;
    dnam[76..80].copy_from_slice(&6_u32.to_le_bytes());
    Record::new(Tag(*b"WEAP"), object)
        .sub(
            Tag(*b"DATA"),
            [&0_u32.to_le_bytes()[..], &1.0_f32.to_le_bytes()[..], &10_u16.to_le_bytes()[..]]
                .concat(),
        )
        .sub(Tag(*b"DNAM"), dnam)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"ARMO"), SHIELD)
            .sub(Tag(*b"DATA"), [20_i32.to_le_bytes(), 1.0_f32.to_le_bytes()].concat())
            .sub(Tag(*b"DNAM"), 100_u32.to_le_bytes())
            .sub(Tag(*b"BOD2"), [(1_u32 << 9).to_le_bytes(), 2_u32.to_le_bytes()].concat()),
    );
    let mut light = vec![0; 32];
    light[12..16].copy_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), TORCH).sub(Tag(*b"DATA"), light));
    plugin.push_top(weapon(SWORD, 1));
    plugin.push_top(weapon(GREATSWORD, 5));
    let mut order = LoadOrder::new();
    order.add("Inv09.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    for item in [SHIELD, SWORD, GREATSWORD, TORCH] {
        world.add_item(player, id(item), 1);
    }
    drop(world);
    game
}

#[test]
fn inv_09_a_two_handed_weapon_removes_the_shield() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.equip_item(player, id(SHIELD)).unwrap();
    assert_eq!(world.equipped_shield(player).unwrap(), Some(id(SHIELD)));
    world.equip_hand_weapon(player, Hand::Right, id(GREATSWORD)).unwrap();
    assert_eq!(world.equipped_shield(player).unwrap(), None);
    assert_eq!(world.actor(player).unwrap().right_weapon, Some(id(GREATSWORD)));
}

#[test]
fn inv_09_a_shield_puts_a_two_handed_weapon_away() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.equip_hand_weapon(player, Hand::Right, id(GREATSWORD)).unwrap();
    world.equip_item(player, id(SHIELD)).unwrap();
    assert_eq!(world.equipped_shield(player).unwrap(), Some(id(SHIELD)));
    assert_eq!(world.actor(player).unwrap().right_weapon, None);
}

#[test]
fn inv_09_a_one_handed_weapon_keeps_the_shield() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.equip_item(player, id(SHIELD)).unwrap();
    world.equip_hand_weapon(player, Hand::Right, id(SWORD)).unwrap();
    assert_eq!(world.equipped_shield(player).unwrap(), Some(id(SHIELD)));
    assert_eq!(world.actor(player).unwrap().right_weapon, Some(id(SWORD)));
}

fn with_torch() -> Game {
    game()
}

#[test]
fn inv_09_a_torch_and_a_shield_take_the_left_hand_from_each_other() {
    let mut game = with_torch();
    let player = game.player();
    let mut world = game.world_mut();
    world.equip_item(player, id(SHIELD)).unwrap();
    world.equip_item(player, id(TORCH)).unwrap();
    assert_eq!(world.equipped_shield(player).unwrap(), None);
    assert!(world.actor(player).unwrap().equipped.contains(&id(TORCH)));
    world.equip_item(player, id(SHIELD)).unwrap();
    assert!(!world.actor(player).unwrap().equipped.contains(&id(TORCH)));
}
