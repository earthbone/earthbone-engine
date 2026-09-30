// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-15 AttackDamageMult composition and modifier validation edge cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::{Error, MeleeHit};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
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
    game
}

fn hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_15_repeated_hits_apply_outgoing_damage_multiplier_once_per_hit() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "AttackDamageMult", 1.5)
        .unwrap();
    let first = game.melee_hit(hit(0x900, 0x901)).unwrap();
    let second = game.melee_hit(hit(0x900, 0x901)).unwrap();
    assert_eq!(first.weapon_damage, 15.0);
    assert_eq!(second.weapon_damage, 15.0);
    assert_eq!(first.health_damage, 15.0);
    assert_eq!(second.health_damage, 15.0);
    assert_eq!(game.world_mut().actor_value(id(0x901), "health"), 70.0);
}

#[test]
fn cmb_15_canonical_marksman_modifier_rejects_without_state_changes() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "MarksmanModifier", 10.0)
        .unwrap();
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.melee_hit(hit(0x900, 0x901)),
        Err(Error::Unsupported("combat actor-value multipliers"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}
