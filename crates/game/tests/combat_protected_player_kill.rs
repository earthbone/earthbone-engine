// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! CMB-34 done check: a protected actor dies to a lethal player hit and
//! enters bleedout from an NPC hit. Mirrors the synthetic-plugin fixtures in
//! combat_bleedout_recovery.rs and combat_hits.rs, but drives `melee_hit`
//! with both a player attacker and a baseless NPC attacker against the same
//! protected target, so both halves of the CMB-34 exception sit in one file.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, npc::flags, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
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
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .sub(Tag(*b"EDID"), name.as_bytes().to_vec())
                .sub(Tag(*b"DATA"), value.to_le_bytes().to_vec()),
        );
    }
}

/// One protected, non-essential target actor (0x900), plus a baseless NPC
/// attacker (0x901) and the player, sharing the weapon fixture the combat
/// hit and CMB-13 bleedout tests use. Both `hit()` variants below reuse this
/// so the only difference between the kill case and the bleedout case is
/// which actor swings.
fn fixture() -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&flags::PROTECTED.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("ProtectedKill.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit_from(attacker: GlobalFormId) -> MeleeHit {
    MeleeHit {
        attacker,
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

/// CMB-34 done check, first clause: a lethal player hit kills a merely
/// protected, non-essential actor outright instead of clamping them into
/// bleedout.
#[test]
fn cmb_34_player_lethal_hit_kills_protected_target() {
    let mut game = fixture();
    // Same 15 health / 15 damage setup as the CMB-13 fixture: one hit is
    // lethal, and the bleedout threshold (3.) sits well above zero, so a
    // clamp instead of a kill would be unambiguous in the assertions below.
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(hit_from(id(0x14))).unwrap();

    assert!(
        result.killed,
        "a lethal player hit must kill a protected, non-essential target"
    );
    let mut world = game.world_mut();
    let target = world.actor(id(0x900)).unwrap();
    assert!(target.dead, "target must be marked dead");
    assert!(
        !target.unconscious,
        "a killed target must not also be left in bleedout"
    );
}

/// CMB-34 done check, second clause: the same protected target still enters
/// bleedout, rather than dying, when the attacker is an NPC and not the
/// player.
#[test]
fn cmb_34_npc_lethal_hit_enters_bleedout_for_protected_target() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(hit_from(id(0x901))).unwrap();

    assert!(
        !result.killed,
        "an NPC hit on a protected target must not kill it"
    );
    let mut world = game.world_mut();
    let target = world.actor(id(0x900)).unwrap();
    assert!(target.unconscious, "target must enter bleedout");
    assert!(!target.dead, "target must not be marked dead");
}

/// The player exception is protected-only: an essential target stays gated
/// into bleedout even when the player lands the lethal hit, matching
/// docs/combat.md's CMB-34 section and the mutually-exclusive protected and
/// essential flags on `ActorBase_Script`.
#[test]
fn cmb_34_player_lethal_hit_does_not_kill_essential_target() {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&flags::ESSENTIAL.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("ProtectedKill.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x900)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(hit_from(player)).unwrap();

    assert!(
        !result.killed,
        "essential target must not die to the player"
    );
    assert!(game.world_mut().actor(id(0x900)).unwrap().unconscious);
}

/// SKYRIM_DATA gated: a real protected, non-essential NPC_ winner from the
/// installed load order takes the CMB-34 exception the same way the
/// synthetic fixture does, on an actual game record rather than hand-built
/// ACBS bytes.
#[test]
fn cmb_34_installed_protected_npc_takes_the_player_exception() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed protected NPC check");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut found = None;
    for (form, record) in order
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
    {
        let Ok(npc) = soe_game::npc::Npc::from_record(record, false) else {
            continue;
        };
        let protected_only =
            npc.config.flags & flags::PROTECTED != 0 && npc.config.flags & flags::ESSENTIAL == 0;
        if protected_only {
            let name = npc.editor_id.clone().unwrap_or_default();
            if !name.is_empty() {
                found = Some((form, name));
                break;
            }
        }
    }
    let (base, name) =
        found.expect("installed load order carries a named protected, non-essential NPC");
    eprintln!("cmb_34: using installed protected NPC {base} {name}");

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    let target = GlobalFormId {
        file: 1,
        object: 0x900,
    };
    patch.push_top(
        Record::new(Tag(*b"ACHR"), target.object).sub(Tag(*b"NAME"), base.object.to_le_bytes()),
    );
    patch.push_top(weapon(0xA00, 10, 6));
    settings(&mut patch);
    order.add("ProtectedSurvey.esp", patch).unwrap();
    let file = order.files().len() - 1;
    let target = GlobalFormId {
        file: file as u16,
        object: 0x900,
    };
    let survey_weapon = GlobalFormId {
        file: file as u16,
        object: 0xA00,
    };

    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 100.).unwrap();
        world.add_item(player, survey_weapon, 1);
        world.actor(player).unwrap().equipped.insert(survey_weapon);
        world.set_actor_value(target, "health", 15.).unwrap();
    }

    let result = game
        .melee_hit(MeleeHit {
            attacker: player,
            target,
            weapon: survey_weapon,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();

    assert!(
        result.killed,
        "a lethal player hit must kill the installed protected, non-essential NPC {name}"
    );
}
