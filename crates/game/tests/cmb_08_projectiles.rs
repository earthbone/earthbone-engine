// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-08 arrow and bolt release transaction.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, combat::Error};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ranged_game(weapon_animation: u8, ammo_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = weapon_animation;
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0x800)
            .sub(Tag(*b"DATA"), weapon_data)
            .sub(Tag(*b"DNAM"), weapon_dnam),
    );
    let mut ammo_data = Vec::new();
    ammo_data.extend_from_slice(&0x802_u32.to_le_bytes());
    ammo_data.extend_from_slice(&ammo_flags.to_le_bytes());
    ammo_data.extend_from_slice(&12_f32.to_le_bytes());
    ammo_data.extend_from_slice(&1_u32.to_le_bytes());
    ammo_data.extend_from_slice(&0.1_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"AMMO"), 0x801).sub(Tag(*b"DATA"), ammo_data));
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"PROJ"), 0x802).sub(Tag(*b"DATA"), projectile));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Ranged.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    world.add_item(player, id(0x800), 1);
    world.add_item(player, id(0x801), 3);
    let state = world.actor(player).unwrap();
    state.right_weapon = Some(id(0x800));
    state.equipped.insert(id(0x800));
    state.equipped.insert(id(0x801));
    drop(world);
    game
}

#[test]
fn cmb_08_release_consumes_one_compatible_projectile_and_creates_flight_request() {
    let mut game = ranged_game(7, 1 << 2);
    let player = game.player();
    let flight = game
        .release_projectile(player, id(0x800), [1.0, 2.0, 3.0], [0.0, 3.0, 4.0])
        .unwrap();
    assert_eq!(flight.attacker, player);
    assert_eq!(flight.weapon, id(0x800));
    assert_eq!(flight.ammunition, id(0x801));
    assert_eq!(flight.projectile, id(0x802));
    assert_eq!(flight.direction, [0.0, 0.6, 0.8]);
    assert_eq!(flight.gravity, 0.1);
    assert_eq!(flight.speed, 3000.0);
    assert_eq!(flight.range, 5000.0);
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 2);
}

#[test]
fn cmb_08_crossbow_rejects_arrow_ammo_without_consuming_it() {
    let mut game = ranged_game(9, 1 << 2);
    let player = game.player();
    let result = game.release_projectile(player, id(0x800), [0.0; 3], [0.0, 0.0, 1.0]);
    assert!(matches!(result, Err(Error::Equipment)));
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 3);
}

#[test]
fn cmb_08_crossbow_consumes_one_compatible_bolt() {
    let mut game = ranged_game(9, 0);
    let player = game.player();
    let flight = game
        .release_projectile(player, id(0x800), [0.0; 3], [1.0, 0.0, 0.0])
        .unwrap();
    assert_eq!(flight.ammunition, id(0x801));
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 2);
}

#[test]
fn cmb_08_invalid_launch_vector_does_not_consume_ammo() {
    let mut game = ranged_game(7, 1 << 2);
    let player = game.player();
    let result = game.release_projectile(player, id(0x800), [0.0; 3], [0.0; 3]);
    assert!(matches!(result, Err(Error::Number)));
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 3);
}
