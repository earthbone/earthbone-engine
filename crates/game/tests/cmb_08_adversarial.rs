// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-08 release edge cases from the local AMMO and PROJ format contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, combat::Error};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ranged_game(count: i32, projectile_kind: u16) -> Game {
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
    projectile[2..4].copy_from_slice(&projectile_kind.to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_bits().to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_bits().to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_bits().to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"PROJ"), 0x802).sub(Tag(*b"DATA"), projectile));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Ranged.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    world.add_item(player, id(0x800), 1);
    world.add_item(player, id(0x801), count);
    let actor = world.actor(player).unwrap();
    actor.right_weapon = Some(id(0x800));
    actor.equipped.insert(id(0x800));
    actor.equipped.insert(id(0x801));
    drop(world);
    game
}

#[test]
fn cmb_08_repeated_releases_consume_exactly_one_round_each() {
    let mut game = ranged_game(2, 1 << 6);
    let player = game.player();
    for remaining in [1, 0] {
        let flight = game
            .release_projectile(player, id(0x800), [0.0; 3], [0.0, 0.0, 2.0])
            .unwrap();
        assert_eq!(flight.projectile, id(0x802));
        assert_eq!(game.world_mut().item_count(player, id(0x801)), remaining);
    }
    assert!(matches!(
        game.release_projectile(player, id(0x800), [0.0; 3], [0.0, 0.0, 1.0]),
        Err(Error::Equipment)
    ));
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 0);
}

#[test]
fn cmb_08_non_arrow_projectile_rejects_before_consuming_ammunition() {
    let mut game = ranged_game(1, 1 << 1);
    let player = game.player();
    assert!(matches!(
        game.release_projectile(player, id(0x800), [0.0; 3], [0.0, 0.0, 1.0]),
        Err(Error::Unsupported(
            "ammunition does not use an arrow projectile"
        ))
    ));
    assert_eq!(game.world_mut().item_count(player, id(0x801)), 1);
}
