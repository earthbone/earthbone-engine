// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the Papyrus `Weapon.Fire` record contract.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(source_flags: u32, ammo_damage: f32, projectile_speed: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x100));
    let mut dnam = vec![0; 100];
    dnam[0] = 7;
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 0x200)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), dnam),
    );
    let mut ammo = vec![0; 20];
    ammo[0..4].copy_from_slice(&0x204_u32.to_le_bytes());
    ammo[4..8].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    ammo[8..12].copy_from_slice(&ammo_damage.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"AMMO"), 0x201).sub(Tag::new(b"DATA"), ammo));
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&projectile_speed.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"PROJ"), 0x204).sub(Tag::new(b"DATA"), projectile));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), 0x300)
            .with_flags(source_flags)
            .sub(Tag::new(b"NAME"), 0x100_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("SCR53-breaker3.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_53_deleted_source_reference_is_rejected_without_a_launch() {
    let game = fixture(flags::DELETED, 12.0, 3000.0);
    let result = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)));
    assert!(result.is_err(), "deleted references cannot be fired from");
    assert!(game.world().weapon_fire_requests.is_empty());
}

#[test]
fn scr_53_non_finite_ammo_damage_is_rejected_without_a_launch() {
    let game = fixture(0, f32::NAN, 3000.0);
    assert!(game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)))
        .is_err());
    assert!(game.world().weapon_fire_requests.is_empty());
}

#[test]
fn scr_53_non_positive_projectile_speed_is_rejected_without_a_launch() {
    let game = fixture(0, 12.0, 0.0);
    assert!(game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)))
        .is_err());
    assert!(game.world().weapon_fire_requests.is_empty());
}

#[test]
fn scr_53_valid_repeated_calls_queue_one_request_per_call() {
    let game = fixture(0, 12.0, 3000.0);
    for _ in 0..3 {
        game.world_mut()
            .fire_weapon(id(0x200), id(0x300), Some(id(0x201)))
            .unwrap();
    }
    assert_eq!(game.world().weapon_fire_requests.len(), 3);
}
