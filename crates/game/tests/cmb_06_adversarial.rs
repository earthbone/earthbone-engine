// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for CMB-06 sneak attack qualification.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::{
    combat::MeleeHit,
    detection::{Motion, Observation},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn gmst_i(object: u32, name: &str, value: i32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
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
    data[4..8].copy_from_slice(&10_f32.to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
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
    for (n, (name, value)) in [
        (0x1000, ("fDamageWeaponMult", 1_f32)),
        (0x1001, ("fDamageSkillMin", 1.)),
        (0x1002, ("fDamageSkillMax", 1.)),
        (0x1003, ("fArmorRatingBase", 1.)),
        (0x1004, ("fArmorRatingPCMax", 1.4)),
        (0x1005, ("fArmorRatingMax", 2.5)),
        (0x1006, ("fArmorScalingFactor", 0.12)),
        (0x1007, ("fArmorBaseFactor", 0.03)),
        (0x1008, ("fMaxArmorRating", 80.)),
        (0x1009, ("fCombatSneak1HSwordMult", 4.)),
        (0x1010, ("fSneakBaseValue", 0.)),
        (0x1011, ("fSneakMaxDistance", 100.)),
        (0x1012, ("fSneakDistanceAttenuationExponent", 1.)),
        (0x1013, ("fSneakSoundsMult", 1.)),
        (0x1014, ("fSneakSoundLosMult", 1.)),
        (0x1015, ("fSneakEquippedWeightBase", 0.)),
        (0x1016, ("fSneakEquippedWeightMult", 1.)),
        (0x1017, ("fSneakRunningMult", 1.)),
        (0x1018, ("fSneakActionMult", 1.)),
        (0x1019, ("fSneakLightMoveMult", 0.)),
        (0x101A, ("fSneakLightRunMult", 0.)),
        (0x101B, ("fDetectionSneakLightMod", 0.)),
        (0x101C, ("fSneakLightMult", 1.)),
        (0x101D, ("fSneakStealthBoyMult", 1.)),
        (0x101E, ("fSneakPerceptionSkillMin", 0.)),
        (0x101F, ("fSneakPerceptionSkillMax", 100.)),
        (0x1020, ("fSneakAlertMod", 0.)),
        (0x1021, ("fSneakCombatMod", 0.)),
        (0x1022, ("fCombatStealthPointMax", 100.)),
        (0x1023, ("fCombatStealthPointDrainMult", 1.)),
        (0x1024, ("fCombatStealthPointRegenMult", 1.)),
        (0x1025, ("fCombatStealthPointRegenMin", 1.)),
        (0x1026, ("fCombatStealthPointRegenAlertWaitTime", 5.)),
        (0x1027, ("fCombatStealthPointRegenLostWaitTime", 5.)),
    ] {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), n)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    plugin.push_top(gmst_i(0x1100, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0x1101,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));
    let mut order = LoadOrder::new();
    order.add("Sneak.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        let mut world = game.world_mut();
        world.set_actor_value(id(actor), "health", 100.).unwrap();
        world.add_item(id(actor), id(0xA00), 1);
        world.actor(id(actor)).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit(bash: bool) -> MeleeHit {
    MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: bash,
        is_power_attack: false,
    }
}

#[test]
fn combat_awareness_stops_sneak_damage_even_while_actor_remains_sneaking() {
    let mut game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().sneaking = true;
    game.world_mut()
        .evaluate_detection(
            &[Observation {
                observer: id(0x901),
                target: id(0x900),
                distance: 0.,
                line_of_sight: true,
                light_level: 100.,
                motion: Motion::Still,
                action_sound: 0.,
                action_sound_distance: None,
                action_sound_line_of_sight: None,
            }],
            0.,
        )
        .unwrap();

    let result = game.melee_hit(hit(false)).unwrap();

    assert!(!result.sneak_attacked);
    assert_eq!(result.health_damage, 10.);
}
