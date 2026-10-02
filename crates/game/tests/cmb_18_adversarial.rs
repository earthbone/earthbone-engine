// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for witnessed combat assaults.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, combat::MeleeHit};

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
    data[4..8].copy_from_slice(&1_f32.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    let mut crva = vec![0, 0, 0, 0];
    crva.extend_from_slice(&40_u16.to_le_bytes());
    crva.extend_from_slice(&[0; 6]);
    plugin.push_top(
        Record::new(Tag(*b"FACT"), 0x701)
            .sub(Tag(*b"DATA"), 0x40_u32.to_le_bytes())
            .sub(Tag(*b"CRVA"), crva),
    );
    for (i, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 1.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.),
        ("fArmorRatingMax", 1.),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + i as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Assault.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    for witness in [id(0x900), id(0x901)] {
        game.world_mut().actor(witness).unwrap().crime_faction = Some(id(0x701));
    }
    game
}

fn hit(target: GlobalFormId) -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target,
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn each_witnessed_assault_adds_the_fact_assault_price() {
    let mut game = fixture();
    game.melee_hit(hit(id(0x900))).unwrap();
    game.melee_hit(hit(id(0x901))).unwrap();

    assert_eq!(
        game.world_mut()
            .faction(id(0x701))
            .unwrap()
            .crime_gold_violent,
        80
    );
    assert!(
        game.world_mut()
            .actor(id(0x900))
            .unwrap()
            .witnessed_crimes
            .contains(&id(0x14))
    );
    assert!(
        game.world_mut()
            .actor(id(0x901))
            .unwrap()
            .witnessed_crimes
            .contains(&id(0x14))
    );
}

#[test]
fn witnessed_assault_and_bounty_survive_engine_save_load() {
    let mut game = fixture();
    game.melee_hit(hit(id(0x901))).unwrap();
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();

    assert_eq!(
        game.world_mut()
            .faction(id(0x701))
            .unwrap()
            .crime_gold_violent,
        40
    );
    assert!(
        game.world_mut()
            .actor(id(0x901))
            .unwrap()
            .witnessed_crimes
            .contains(&id(0x14))
    );
}
