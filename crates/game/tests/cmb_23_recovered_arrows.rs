// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-23 recovered arrow identity and save round trip.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ranged_game(ammo_flags: u32, ammo_record_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 7;
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
    let mut ammo = Record::new(Tag(*b"AMMO"), 0x801).sub(Tag(*b"DATA"), ammo_data);
    ammo.flags = ammo_record_flags;
    plugin.push_top(ammo);
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"PROJ"), 0x802).sub(Tag(*b"DATA"), projectile));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Cmb23.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    world.add_item(player, id(0x800), 1);
    world.add_item(player, id(0x801), 3);
    let actor = world.actor(player).unwrap();
    actor.right_weapon = Some(id(0x800));
    actor.equipped.insert(id(0x800));
    actor.equipped.insert(id(0x801));
    drop(world);
    game
}

#[test]
fn cmb_23_recovered_arrow_identity_and_ammunition_survive_engine_save_load() {
    let mut game = ranged_game(1 << 2, 0);
    let player = game.player();
    let request = game
        .release_projectile(player, id(0x800), [0.0; 3], [0.0, 1.0, 0.0])
        .unwrap();
    assert!(request.recoverable);
    let impact = [12.0, 34.0, 56.0];
    let arrow = game
        .world_mut()
        .place_recovered_arrow(&request, impact)
        .unwrap();

    let saved = game.save_state().unwrap();
    let mut loaded = ranged_game(1 << 2, 0);
    loaded.load_state(&saved).unwrap();
    let world = loaded.world_mut();
    let recovered = world.reference_ref(arrow).unwrap();
    assert_eq!(recovered.base, Some(id(0x801)));
    assert_eq!(recovered.position, impact);
    assert_eq!(arrow.file, soe_game::world::DYNAMIC_FILE);
}

#[test]
fn cmb_23_nonplayable_arrow_does_not_create_a_recoverable_reference() {
    let mut game = ranged_game((1 << 2) | (1 << 1), 0);
    let player = game.player();
    let request = game
        .release_projectile(player, id(0x800), [0.0; 3], [0.0, 1.0, 0.0])
        .unwrap();
    assert!(!request.recoverable);
    assert!(game
        .world_mut()
        .place_recovered_arrow(&request, [12.0, 34.0, 56.0])
        .is_none());
}

#[test]
fn cmb_23_nonplayable_ammo_record_does_not_create_a_recoverable_reference() {
    let mut game = ranged_game(1 << 2, 1 << 2);
    let player = game.player();
    let request = game
        .release_projectile(player, id(0x800), [0.0; 3], [0.0, 1.0, 0.0])
        .unwrap();
    assert!(!request.recoverable);
    assert!(game
        .world_mut()
        .place_recovered_arrow(&request, [12.0, 34.0, 56.0])
        .is_none());
}
