// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Weapon skill use from player melee hits.
//!
//! UESP `Skyrim:Leveling` lists the skill use of One-handed and Two-handed
//! as the base weapon damage per hit, and Archery as the base damage of the
//! bow. `World::melee_hit` passes that base damage to `advance_skill`, which
//! applies the AVIF use multiplier and offset. Shield bashes and unarmed
//! contacts never reach the weapon branch, and NPC attackers do not gain
//! experience.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Difficulty, MeleeHit},
    Game,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A one-handed WEAP with the DNAM offsets of `cmb_14_stagger.rs`.
fn weapon(object: u32, damage: u16) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1; // animation: one-hand sword
    put(&mut dnam, 4, 1_f32.to_bits()); // speed
    put(&mut dnam, 8, 1.1_f32.to_bits()); // reach
    put(&mut dnam, 76, 6); // skill: one-handed
    put(&mut dnam, 88, u32::MAX); // resist: -1 (no resistance type)
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn settings(plugin: &mut Plugin) {
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
        ("fSkillUseCurve", 2.),
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
}

/// The player and two ACHR actors, all holding a 12-damage one-handed sword,
/// plus an AVOneHanded AVIF whose AVSK use multiplier is 1 and offset 0.
fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 12));
    plugin.push_top(
        Record::new(Tag(*b"AVIF"), 0x801)
            .zsub(Tag(*b"EDID"), "AVOneHanded")
            .sub(
                Tag(*b"AVSK"),
                [1.0f32, 0.0, 1.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<u8>>(),
            ),
    );
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("SkillUse.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 1000.).unwrap();
        world.set_actor_value(actor, "onehanded", 20.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game.set_difficulty(Difficulty::Adept);
    game
}

fn hit(attacker: GlobalFormId, target: u32) -> MeleeHit {
    MeleeHit {
        attacker,
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

fn experience(game: &Game, actor: GlobalFormId) -> f32 {
    game.world().skill_experience(actor, "onehanded").unwrap()
}

#[test]
fn player_melee_hit_trains_the_weapon_skill_by_base_damage() {
    let mut game = fixture();
    let player = game.player();
    assert_eq!(experience(&game, player), 0.0);

    let result = game.melee_hit(hit(player, 0x900)).unwrap();
    assert!(result.health_damage > 0.0, "the fixture hit must land");
    let gained = experience(&game, player);
    assert!(
        (gained - 12.0).abs() < 1e-4,
        "one hit with a 12-damage sword is 12 uses at multiplier 1, got {gained}"
    );

    game.melee_hit(hit(player, 0x900)).unwrap();
    let doubled = experience(&game, player);
    assert!(
        (doubled - 24.0).abs() < 1e-4,
        "a second hit adds another 12 uses, got {doubled}"
    );
}

#[test]
fn npc_hits_do_not_train_the_weapon_skill() {
    let mut game = fixture();
    let player = game.player();

    game.melee_hit(hit(id(0x900), 0x901)).unwrap();
    assert_eq!(
        experience(&game, player),
        0.0,
        "an NPC hit must not reach the player's experience"
    );
    assert_eq!(
        experience(&game, id(0x900)),
        0.0,
        "NPC attackers do not gain skill experience"
    );
}
