// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-40 arrow and bolt contact with an actor: damage from the bow plus the
//! AMMO, the marksman scale and the shared hit path.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_le_bytes());
    weapon_data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 7;
    weapon_dnam[76..80].copy_from_slice(&8_u32.to_le_bytes());
    weapon_dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
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
    projectile[0..2].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"PROJ"), 0x802).sub(Tag(*b"DATA"), projectile));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x803));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes()));
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Cmb40.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    world.add_item(player, id(0x800), 1);
    world.add_item(player, id(0x801), 3);
    world.set_actor_value(player, "marksman", 15.).unwrap();
    world.set_actor_value(id(0x900), "health", 100.).unwrap();
    let actor = world.actor(player).unwrap();
    actor.right_weapon = Some(id(0x800));
    actor.equipped.insert(id(0x800));
    actor.equipped.insert(id(0x801));
    drop(world);
    game
}

#[test]
fn cmb_40_an_arrow_contact_damages_the_actor_with_bow_plus_ammo_damage() {
    let mut game = game();
    let player = game.player();
    let request = game
        .release_projectile(player, id(0x800), [0.0; 3], [0.0, 1.0, 0.0])
        .unwrap();
    let before = game
        .world_mut()
        .actor(id(0x900))
        .unwrap()
        .values
        .get("health")
        .current();
    let result = game.projectile_hit(&request, id(0x900)).unwrap();
    let after = game
        .world_mut()
        .actor(id(0x900))
        .unwrap()
        .values
        .get("health")
        .current();
    assert!(
        result.weapon_damage >= 22.0,
        "bow 10 plus arrow 12: {}",
        result.weapon_damage
    );
    assert!(result.health_damage > 0.0);
    assert!(
        (before - after - result.health_damage).abs() < 1e-3,
        "{before} {after}"
    );
    assert!(!result.bashed && !result.sneak_attacked);
}

#[test]
fn cmb_40_a_contact_needs_the_shooter_to_still_hold_the_bow() {
    let mut game = game();
    let player = game.player();
    let request = game
        .release_projectile(player, id(0x800), [0.0; 3], [0.0, 1.0, 0.0])
        .unwrap();
    game.world_mut()
        .actor(player)
        .unwrap()
        .equipped
        .remove(&id(0x800));
    assert!(game.projectile_hit(&request, id(0x900)).is_err());
}
