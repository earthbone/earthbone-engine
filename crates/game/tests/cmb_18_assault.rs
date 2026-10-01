// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-18 witnessed assault bounty behavior.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon() -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1_f32.to_bits());
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    Record::new(Tag(*b"WEAP"), 0xA00)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn crime_faction(flags: u32) -> Record {
    let mut crva = vec![0, 0];
    crva.extend_from_slice(&0_u16.to_le_bytes());
    crva.extend_from_slice(&40_u16.to_le_bytes());
    crva.extend_from_slice(&[0; 6]);
    Record::new(Tag(*b"FACT"), 0x701)
        .sub(Tag(*b"DATA"), flags.to_le_bytes())
        .sub(Tag(*b"CRVA"), crva)
}

fn fixture(faction_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon());
    plugin.push_top(crime_faction(faction_flags));
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
    order.add("CombatCrime.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game.world_mut().actor(id(0x901)).unwrap().crime_faction = Some(id(0x701));
    game
}

fn hit(attacker: GlobalFormId, target: GlobalFormId) -> MeleeHit {
    MeleeHit {
        attacker,
        target,
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_18_witnessed_player_assault_charges_owning_faction() {
    let mut game = fixture(0x40);
    let player = id(0x14);

    game.melee_hit(hit(player, id(0x901))).unwrap();

    let mut world = game.world_mut();
    assert_eq!(world.faction(id(0x701)).unwrap().crime_gold_violent, 40);
    assert!(world
        .actor(id(0x901))
        .unwrap()
        .witnessed_crimes
        .contains(&player));
}

#[test]
fn cmb_18_ignore_assault_flag_leaves_bounty_unchanged() {
    let mut game = fixture(0x40 | 0x100);

    game.melee_hit(hit(id(0x14), id(0x901))).unwrap();

    assert_eq!(
        game.world_mut()
            .faction(id(0x701))
            .unwrap()
            .crime_gold_violent,
        0
    );
}

#[test]
fn cmb_18_non_player_combat_does_not_report_player_assault() {
    let mut game = fixture(0x40);

    game.melee_hit(hit(id(0x900), id(0x901))).unwrap();

    assert_eq!(
        game.world_mut()
            .faction(id(0x701))
            .unwrap()
            .crime_gold_violent,
        0
    );
}

#[test]
fn cmb_18_lethal_hit_does_not_price_assault() {
    let mut game = fixture(0x40);
    game.world_mut()
        .set_actor_value(id(0x901), "health", 1.)
        .unwrap();

    let result = game.melee_hit(hit(id(0x14), id(0x901))).unwrap();

    assert!(result.killed);
    assert_eq!(
        game.world_mut()
            .faction(id(0x701))
            .unwrap()
            .crime_gold_violent,
        0
    );
}
