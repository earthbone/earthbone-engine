// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-08 host release and cancellation latch.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::combat::RangedRelease;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ranged_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 7;
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0x800)
            .sub(Tag(*b"DATA"), weapon_data)
            .sub(Tag(*b"DNAM"), weapon_dnam),
    );
    let mut ammo_data = Vec::new();
    ammo_data.extend_from_slice(&0x802_u32.to_le_bytes());
    ammo_data.extend_from_slice(&(1_u32 << 2).to_le_bytes());
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
fn cmb_08_cancelled_release_consumes_no_ammunition() {
    let mut game = ranged_game();
    let player = game.player();
    let mut release = RangedRelease::default();
    release.begin(&mut game, player).unwrap();
    release.cancel();
    assert!(release
        .event(&mut game, "bowRelease", [0.0; 3], [0.0, 0.0, 1.0])
        .unwrap()
        .is_none());
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 3);
}

#[test]
fn cmb_08_release_event_returns_flight_request_once() {
    let mut game = ranged_game();
    let player = game.player();
    let mut release = RangedRelease::default();
    release.begin(&mut game, player).unwrap();
    let request = release
        .event(&mut game, "bowRelease", [0.0; 3], [0.0, 0.0, 1.0])
        .unwrap()
        .unwrap();
    assert_eq!(request.ammunition, id(0x801));
    assert!(release
        .event(&mut game, "bowRelease", [0.0; 3], [0.0, 0.0, 1.0])
        .unwrap()
        .is_none());
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 2);
}
