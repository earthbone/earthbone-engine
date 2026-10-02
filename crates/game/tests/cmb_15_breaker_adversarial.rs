// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-15 validation when a supported PERK actor-value multiplier is negative.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::combat::{Error, MeleeHit};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture_with_negative_modifier() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_le_bytes());
    weapon_data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), weapon_data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.0),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 1.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.0),
        ("fArmorRatingMax", 1.0),
        ("fArmorScalingFactor", 0.0),
        ("fArmorBaseFactor", 0.0),
        ("fMaxArmorRating", 80.0),
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
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&96_f32.to_le_bytes());
    data.extend_from_slice(&(-1.0_f32).to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"PERK"), 0xB10)
            .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 0, 0])
            .sub(Tag(*b"DATA"), [35, 14, 3])
            .sub(Tag(*b"PRKC"), [0])
            .sub(Tag(*b"PRKC"), [1])
            .sub(Tag(*b"PRKC"), [2])
            .sub(Tag(*b"EPFT"), [2])
            .sub(Tag(*b"EPFD"), data)
            .sub(Tag(*b"PRKF"), []),
    );
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "onehanded", 100.0)
            .unwrap();
        game.world_mut().add_item(id(actor), id(0xA00), 1);
        game.world_mut()
            .actor(id(actor))
            .unwrap()
            .equipped
            .insert(id(0xA00));
    }
    game.world_mut()
        .set_actor_value(id(0x900), "onehandedmodifier", 20.0)
        .unwrap();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .perks
        .insert(id(0xB10));
    game
}

#[test]
fn cmb_15_negative_perk_scaled_damage_rejects_without_healing_or_events() {
    let mut game = fixture_with_negative_modifier();
    let result = game.melee_hit(MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    });
    assert!(matches!(result, Err(Error::Number)));
    assert_eq!(game.world_mut().actor_value(id(0x901), "health"), 100.0);
}
