// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Active Invisibility must reach the shared detection calculation.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{Motion, Observation};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const OBSERVER: u32 = 0x100;
const TARGET: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [OBSERVER, TARGET] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    let mut mgef = vec![0; 152];
    mgef[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    mgef[64..68].copy_from_slice(&11_u32.to_le_bytes());
    mgef[80..84].copy_from_slice(&1_u32.to_le_bytes());
    mgef[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));
    let mut spit = vec![0; 36];
    spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 0.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&30_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let gmsts: [(&str, f32); 24] = [
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
        ("fSneakStealthBoyMult", 0.1),
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
    ];
    for (index, (name, value)) in gmsts.into_iter().enumerate() {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x1000 + index as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    for (index, (name, value)) in [
        ("iCombatStealthPointDetectionThreshold", 50_i32),
        ("iCombatStealthPointSneakDetectionThreshold", 70_i32),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x1100 + index as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    Game::new(order)
}

fn observation() -> Observation {
    Observation {
        observer: id(OBSERVER),
        target: id(TARGET),
        distance: 0.0,
        line_of_sight: true,
        light_level: 1.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    }
}

#[test]
fn mag_34_active_invisibility_reduces_the_detection_value() {
    let visible = game();
    visible
        .world_mut()
        .evaluate_detection(&[observation()], 0.0)
        .unwrap();
    let visible_value = visible
        .world()
        .awareness(id(OBSERVER), id(TARGET))
        .unwrap()
        .detection;

    let hidden = game();
    hidden
        .shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(TARGET),
            Some(id(TARGET)),
            Some(id(TARGET)),
            &mut |_, _| soe_papyrus::ObjectId(1),
        )
        .unwrap();
    assert!(hidden.world().is_invisible(id(TARGET)));
    hidden
        .world_mut()
        .evaluate_detection(&[observation()], 0.0)
        .unwrap();
    let hidden_value = hidden
        .world()
        .awareness(id(OBSERVER), id(TARGET))
        .unwrap()
        .detection;
    assert!(
        hidden_value < visible_value,
        "invisibility must lower detection: {visible_value} to {hidden_value}"
    );
}
