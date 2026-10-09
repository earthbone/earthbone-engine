// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_53_valid_weapon_fire_builds_a_record_backed_projectile_request() {
    let mut dnam = vec![0; 100];
    dnam[0] = 7;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 1)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), dnam),
    );
    let mut ammo = vec![0; 20];
    ammo[0..4].copy_from_slice(&5_u32.to_le_bytes());
    ammo[4..8].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    ammo[8..12].copy_from_slice(&12_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"AMMO"), 2).sub(Tag::new(b"DATA"), ammo));
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"PROJ"), 5).sub(Tag::new(b"DATA"), projectile));
    plugin.push_top(Record::new(Tag::new(b"STAT"), 3));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 4).sub(Tag::new(b"NAME"), 3_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order
        .add("scr53-breaker.esp", plugin)
        .expect("plugin loads");
    let game = Game::new(order);
    game.world_mut()
        .fire_weapon(id(1), id(4), Some(id(2)))
        .unwrap();
    let request = game.world().weapon_fire_requests[0];
    assert_eq!(request.weapon, id(1));
    assert_eq!(request.attacker, id(4));
    assert_eq!(request.ammunition, id(2));
    assert_eq!(request.projectile, id(5));
    assert_eq!(request.damage, 12.0);
}
