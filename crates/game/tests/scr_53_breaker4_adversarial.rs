// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(source_flags: u32, weapon_flags: u32, ammo_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x100));
    let mut weapon_data = vec![0; 100];
    weapon_data[0] = 7;
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 0x200)
            .with_flags(weapon_flags)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), weapon_data),
    );
    let mut ammo_data = vec![0; 20];
    ammo_data[0..4].copy_from_slice(&0x204_u32.to_le_bytes());
    ammo_data[4..8].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    ammo_data[8..12].copy_from_slice(&12_f32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag::new(b"AMMO"), 0x201)
            .with_flags(ammo_flags)
            .sub(Tag::new(b"DATA"), ammo_data),
    );
    let mut projectile_data = vec![0; 92];
    projectile_data[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile_data[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile_data[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile_data[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"PROJ"), 0x204).sub(Tag::new(b"DATA"), projectile_data));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), 0x300)
            .with_flags(source_flags)
            .sub(Tag::new(b"NAME"), 0x100_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr53Breaker4.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_53_deleted_source_reference_is_rejected_without_queuing_a_launch() {
    let game = game(flags::DELETED, 0, 0);
    let result = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)));
    assert!(result.is_err());
    assert!(game.world().weapon_fire_requests.is_empty());
}

#[test]
fn scr_53_deleted_weapon_is_rejected_without_queuing_a_launch() {
    let game = game(0, flags::DELETED, 0);
    let result = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)));
    assert!(result.is_err());
    assert!(game.world().weapon_fire_requests.is_empty());
}

#[test]
fn scr_53_deleted_ammunition_is_rejected_without_queuing_a_launch() {
    let game = game(0, 0, flags::DELETED);
    let result = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)));
    assert!(result.is_err());
    assert!(game.world().weapon_fire_requests.is_empty());
}
