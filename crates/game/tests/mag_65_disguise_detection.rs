// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Disguise magic effects and their temporary detection interpretation.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{Motion, Observation};
use soe_game::world::magic::EffectId;
use soe_game::Game;

const OBSERVER: u32 = 0x900;
const TARGET: u32 = 0x901;
const SOURCE: u32 = 0x902;
const EFFECT: u32 = 0xA00;
const SPELL: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [OBSERVER, TARGET, SOURCE] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }

    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 64, 44);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT).sub(Tag(*b"DATA"), effect_data));

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let efit = [
        20.0_f32.to_le_bytes(),
        0_u32.to_le_bytes(),
        3_u32.to_le_bytes(),
    ]
    .concat();
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), efit),
    );

    let float_settings = [
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
    ];
    for (index, (name, value)) in float_settings.into_iter().enumerate() {
        plugin.push_top(gmst_f(0xB00 + index as u32, name, value));
    }
    plugin.push_top(gmst_i(0xB20, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0xB21,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));

    let mut order = LoadOrder::new();
    order.add("Disguise.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [OBSERVER, TARGET, SOURCE] {
        game.world_mut()
            .set_actor_value(id(actor), "sneak", 0.0)
            .unwrap();
    }
    game
}

fn cast_disguise(game: &mut Game) -> EffectId {
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    applied[0]
}

fn detection(game: &mut Game) -> f32 {
    game.world_mut()
        .evaluate_detection(
            &[Observation {
                observer: id(OBSERVER),
                target: id(TARGET),
                distance: 0.0,
                line_of_sight: true,
                light_level: 50.0,
                motion: Motion::Still,
                action_sound: 0.0,
                action_sound_distance: None,
                action_sound_line_of_sight: None,
            }],
            0.0,
        )
        .unwrap();
    game.world()
        .awareness(id(OBSERVER), id(TARGET))
        .unwrap()
        .detection
}

#[test]
fn mag_65_disguise_lowers_detection_for_its_active_lifetime() {
    let mut game = fixture();
    let baseline = detection(&mut game);

    cast_disguise(&mut game);

    assert_eq!(game.world().active_effects().count(), 1);
    let disguised = detection(&mut game);
    assert!(
        (baseline - disguised - 20.0).abs() < 1e-3,
        "{baseline} -> {disguised}"
    );
}

#[test]
fn mag_65_disguise_detection_returns_to_baseline_on_expiry() {
    let mut game = fixture();
    let baseline = detection(&mut game);
    cast_disguise(&mut game);
    let disguised = detection(&mut game);
    assert!(disguised < baseline, "{baseline} -> {disguised}");

    game.tick(3.0);

    assert!(game.world().active_effects().next().is_none());
    let expired = detection(&mut game);
    assert!((expired - baseline).abs() < 1e-3, "{baseline} -> {expired}");
}
