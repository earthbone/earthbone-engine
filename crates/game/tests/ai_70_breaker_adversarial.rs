// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for AI-70 reference state conditions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{
    conditions,
    detection::{Motion, Observation},
    Game,
};

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
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x1200).zsub(Tag(*b"EDID"), "MagicDisease"));
    let mut effect_data = vec![0; 152];
    effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect_data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), 0x1201)
            .sub(Tag(*b"DATA"), effect_data)
            .sub(Tag(*b"KWDA"), 0x1200_u32.to_le_bytes()),
    );
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut spell_effect = 0.0_f32.to_le_bytes().to_vec();
    spell_effect.extend_from_slice(&0_u32.to_le_bytes());
    spell_effect.extend_from_slice(&10_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), 0x1202)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), 0x1201_u32.to_le_bytes())
            .sub(Tag(*b"EFIT"), spell_effect),
    );
    plugin.push_top(Record::new(Tag(*b"GMST"), 0x1000).zsub(Tag(*b"EDID"), "fSneakBaseValue").sub(Tag(*b"DATA"), 0.0_f32.to_le_bytes()));
    let floats: [(&str, f32); 23] = [
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
    ];
    for (index, (name, value)) in floats.into_iter().enumerate() {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1001 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    for (object, name, value) in [
        (0x1100, "iCombatStealthPointDetectionThreshold", 50_i32),
        (0x1101, "iCombatStealthPointSneakDetectionThreshold", 70_i32),
    ] {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), object)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("AI70Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, parameter: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [parameter, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

fn observation(observer: u32, target: u32, line_of_sight: bool) -> Observation {
    Observation {
        observer: id(observer),
        target: id(target),
        distance: 0.0,
        line_of_sight,
        light_level: 100.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    }
}

#[test]
fn ai_70_los_is_directional_and_empty_successful_batch_clears_old_pairs() {
    let game = fixture();
    game.world_mut()
        .evaluate_detection(&[observation(0x901, 0x900, true)], 0.0)
        .unwrap();
    assert_eq!(evaluate(&game, id(0x901), &condition(27, 0x900, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(27, 0x901, 1.0)), Ok(false));
    game.world_mut().evaluate_detection(&[], 0.0).unwrap();
    assert_eq!(evaluate(&game, id(0x901), &condition(27, 0x900, 0.0)), Ok(true));
}

#[test]
fn ai_70_invalid_los_batch_does_not_replace_last_successful_observation() {
    let game = fixture();
    game.world_mut()
        .evaluate_detection(&[observation(0x901, 0x900, true)], 0.0)
        .unwrap();
    let mut invalid = observation(0x901, 0x900, false);
    invalid.distance = f32::NAN;
    assert!(game.world_mut().evaluate_detection(&[invalid], 0.0).is_err());
    assert_eq!(evaluate(&game, id(0x901), &condition(27, 0x900, 1.0)), Ok(true));
}

#[test]
fn ai_70_menu_mode_comparison_and_random_percent_parameter_slots_are_checked() {
    let game = fixture();
    game.world_mut().set_menu_mode(true);
    let mut above_zero = condition(36, 0, 0.0);
    above_zero.flags = 2 << 5;
    assert_eq!(evaluate(&game, id(0x900), &above_zero), Ok(true));
    for slot in [
        Condition { parameters: [1, 0], ..condition(77, 0, 0.0) },
        Condition { parameters: [0, 1], ..condition(77, 0, 0.0) },
    ] {
        assert_eq!(evaluate(&game, id(0x900), &slot), Err(conditions::Error::Parameter(77, 1)));
    }
    for _ in 0..128 {
        let mut below_one_hundred = condition(77, 0, 100.0);
        below_one_hundred.flags = 4 << 5;
        assert_eq!(evaluate(&game, id(0x900), &below_one_hundred), Ok(true));
        let mut at_least_zero = condition(77, 0, 0.0);
        at_least_zero.flags = 3 << 5;
        assert_eq!(evaluate(&game, id(0x900), &at_least_zero), Ok(true));
    }
}

#[test]
fn ai_70_disease_query_reads_active_effect_after_engine_save_load() {
    let mut game = fixture();
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(0x1202),
            id(0x900),
            Some(id(0x900)),
            Some(id(0x900)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(evaluate(&game, id(0x900), &condition(39, 0, 1.0)), Ok(true));
    let bytes = game.world_mut().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(evaluate(&restored, id(0x900), &condition(39, 0, 1.0)), Ok(true));
}
