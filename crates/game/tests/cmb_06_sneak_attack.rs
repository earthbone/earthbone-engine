// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Checks CMB-06 sneak damage against the weapon category GMST and CMB-31
//! awareness state.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::combat::MeleeHit;
use soe_game::detection::{Motion, Observation};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn gmst_f(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn gmst_i(object: u32, name: &str, value: i32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn weapon(animation: u8) -> Record {
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&1_f32.to_le_bytes());
    data[8..10].copy_from_slice(&20_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = animation;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1.1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    Record::new(Tag(*b"WEAP"), 0xA00)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn fixture(animation: u8) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x700));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"RNAM"), 0x700_u32.to_le_bytes()));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(animation));
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 1.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.0),
        ("fCombatSneakHandMult", 8.0),
        ("fCombatSneak1HSwordMult", 2.0),
        ("fCombatSneak1HDaggerMult", 3.0),
        ("fCombatSneak1HAxeMult", 4.0),
        ("fCombatSneak1HMaceMult", 5.0),
        ("fCombatSneak2HSwordMult", 6.0),
        ("fCombatSneak2HAxeMult", 7.0),
        ("fSneakBaseValue", 0.0),
        ("fSneakMaxDistance", 100.0),
        ("fSneakDistanceAttenuationExponent", 1.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 0.0),
        ("fSneakEquippedWeightMult", 1.0),
        ("fSneakRunningMult", 1.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.0),
        ("fSneakLightRunMult", 0.0),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 1.0),
        ("fSneakStealthBoyMult", 1.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 100.0),
        ("fSneakAlertMod", 0.0),
        ("fSneakCombatMod", 0.0),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 5.0),
        ("fCombatStealthPointRegenLostWaitTime", 5.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(gmst_f(0x1000 + index as u32, name, value));
    }
    plugin.push_top(gmst_i(0x1100, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0x1101,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));

    let mut order = LoadOrder::new();
    order.add("SneakAttack.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 1000.0).unwrap();
        world.set_actor_value(actor, "onehanded", 0.0).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_06_sneaking_hit_uses_the_weapon_category_gmst() {
    for (animation, multiplier) in [(1, 2.0), (2, 3.0), (3, 4.0), (4, 5.0), (5, 6.0), (6, 7.0)] {
        let mut game = fixture(animation);
        game.world_mut().actor(id(0x900)).unwrap().sneaking = true;
        let result = game.melee_hit(hit()).unwrap();
        assert!(result.sneak_attacked, "animation category {animation}");
        assert_eq!(result.health_damage, 20.0 * multiplier);
    }
}

#[test]
fn cmb_06_target_combat_awareness_clears_sneak_multiplier() {
    let mut game = fixture(1);
    game.world_mut().actor(id(0x900)).unwrap().sneaking = true;
    game.world_mut()
        .evaluate_detection(
            &[Observation {
                observer: id(0x901),
                target: id(0x900),
                distance: 0.0,
                line_of_sight: true,
                light_level: 100.0,
                motion: Motion::Still,
                action_sound: 0.0,
                action_sound_distance: None,
                action_sound_line_of_sight: None,
            }],
            0.0,
        )
        .unwrap();

    let result = game.melee_hit(hit()).unwrap();

    assert!(
        !result.sneak_attacked,
        "Combat awareness must suppress sneak damage"
    );
    assert_eq!(result.health_damage, 20.0);
}

#[test]
fn cmb_06_non_sneaking_attack_does_not_read_sneak_multiplier() {
    let mut game = fixture(1);

    let result = game.melee_hit(hit()).unwrap();

    assert!(!result.sneak_attacked, "attacker is not sneaking");
    assert_eq!(result.health_damage, 20.0);
}

#[test]
fn cmb_06_installed_skyrim_has_each_melee_sneak_gmst() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed sneak GMST check");
        return;
    };
    let plugin = Plugin::parse(&std::fs::read(data.join("Skyrim.esm")).unwrap()).unwrap();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let game = Game::new(order);

    for name in [
        "fCombatSneakHandMult",
        "fCombatSneak1HSwordMult",
        "fCombatSneak1HDaggerMult",
        "fCombatSneak1HAxeMult",
        "fCombatSneak1HMaceMult",
        "fCombatSneak2HSwordMult",
        "fCombatSneak2HAxeMult",
    ] {
        assert!(game.world().game_setting(name).is_some(), "missing {name}");
    }
}
