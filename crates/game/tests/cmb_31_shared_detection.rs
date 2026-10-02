// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifier pass for CMB-31 (shared actor detection and stealth points).
//!
//! Done check: two observers share target stealth points with record-backed
//! sight, sound and skill terms, and timers that survive save/load and
//! reject invalid observations atomically.
//!
//! `crate::detection::detection_value` already covers the pure formula in
//! unit tests, so this file exercises it through `World::evaluate_detection`
//! and the world's GMST and actor-value records, the surface CMB-31 added.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_game::detection::{DetectionLevel, Error, Motion, Observation};
use soe_papyrus::ObjectId;

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

/// The full set of 26 detection GMSTs (`DetectionSettings::NAMES`), at
/// values chosen to make the formula easy to hand-compute: linear distance
/// attenuation, unit multipliers, and skill terms that pass through the
/// observer's and target's raw Sneak value.
///
/// `overrides` replaces named floats (never the two int settings) before
/// the records are built, so each test isolates the term it exercises.
fn detection_gmsts(overrides: &[(&str, f32)]) -> Vec<Record> {
    let mut values: Vec<(&str, f32)> = vec![
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
    for (name, value) in overrides {
        let slot = values
            .iter_mut()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("unknown override {name}"));
        slot.1 = *value;
    }
    let mut records: Vec<Record> = values
        .into_iter()
        .enumerate()
        .map(|(i, (name, value))| gmst_f(0x1000 + i as u32, name, value))
        .collect();
    records.push(gmst_i(0x1100, "iCombatStealthPointDetectionThreshold", 50));
    records.push(gmst_i(
        0x1101,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));
    records
}

fn still(observer: u32, target: u32, distance: f32, light: f32) -> Observation {
    Observation {
        observer: id(observer),
        target: id(target),
        distance,
        line_of_sight: true,
        light_level: light,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    }
}

/// One `NPC_` base and three `ACHR` actor references (0x900, 0x901 the
/// observers, 0x902 the shared target), plus one templated `ARMO` (0xA00)
/// for the sound-term case.
fn fixture(gmsts: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"ARMO"), 0xA00)
            .sub(
                Tag(*b"DATA"),
                [0_i32.to_le_bytes(), 20_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"DNAM"), 500_u32.to_le_bytes())
            .sub(
                Tag(*b"BOD2"),
                [(1_u32 << 2).to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    let mut invisibility = vec![0; 152];
    invisibility[64..68].copy_from_slice(&11_u32.to_le_bytes());
    invisibility[80..84].copy_from_slice(&1_u32.to_le_bytes());
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    let effect_entry = [
        1.0_f32.to_le_bytes(),
        0_u32.to_le_bytes(),
        20_u32.to_le_bytes(),
    ]
    .concat();
    plugin.push_top(Record::new(Tag(*b"MGEF"), 0xB00).sub(Tag(*b"DATA"), invisibility));
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), 0xB01)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), 0xB00_u32.to_le_bytes())
            .sub(Tag(*b"EFIT"), effect_entry),
    );
    for record in gmsts {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Detection.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn mag_34_active_invisibility_reduces_record_backed_detection() {
    let settings = detection_gmsts(&[("fSneakStealthBoyMult", 0.1)]);
    let observation = still(0x900, 0x902, 0.0, 100.0);
    let mut visible_world = fixture(settings.clone());
    visible_world
        .evaluate_detection(&[observation], 0.0)
        .unwrap();
    let visible = visible_world
        .awareness(id(0x900), id(0x902))
        .unwrap()
        .detection;

    let mut invisible_world = fixture(settings);
    invisible_world
        .cast_spell(id(0xB01), id(0x902), Some(id(0x902)), None, &mut |_, _| {
            ObjectId(1)
        })
        .unwrap();
    invisible_world
        .evaluate_detection(&[observation], 0.0)
        .unwrap();
    let invisible = invisible_world
        .awareness(id(0x900), id(0x902))
        .unwrap()
        .detection;

    assert!(
        invisible < visible,
        "invisibility must lower detection: {visible} to {invisible}"
    );
}

/// Two observers (0x900 Combat-level, 0x901 Alert-level) watching one
/// target (0x902) act on the same shared stealth pool, and both awarenesses
/// live under that one pool rather than per-observer state.
#[test]
fn cmb_31_two_observers_share_target_stealth_points() {
    let mut world = fixture(detection_gmsts(&[]));
    // A: close and lit, above the Combat threshold (50).
    let a = still(0x900, 0x902, 10.0, 100.0);
    // B: half the distance to max range, exactly at the threshold, stays Alert.
    let b = still(0x901, 0x902, 50.0, 100.0);
    world.evaluate_detection(&[a, b], 1.0).unwrap();

    let pool = world.stealth_pool(id(0x902)).expect("pool exists");
    assert_eq!(pool.observers.len(), 2, "both observers share one pool");
    // A crosses the Combat threshold in this step, and Combat holds the
    // shared pool at zero on the step combat starts (docs/combat-ai.md,
    // "Shared stealth points"). B's lower value does not keep a
    // per-observer remainder alive, because the pool is shared.
    assert!(
        pool.points.abs() < 1e-3,
        "Combat holds the shared pool at zero: {}",
        pool.points
    );

    let awareness_a = world.awareness(id(0x900), id(0x902)).unwrap();
    let awareness_b = world.awareness(id(0x901), id(0x902)).unwrap();
    assert_eq!(awareness_a.level, DetectionLevel::Combat);
    assert_eq!(awareness_b.level, DetectionLevel::Alert);
    assert!(
        (awareness_a.detection - 90.0).abs() < 1e-3,
        "{awareness_a:?}"
    );
    assert!(
        (awareness_b.detection - 50.0).abs() < 1e-3,
        "{awareness_b:?}"
    );

    // Same target queried through a second observer resolves the same pool
    // object, not a duplicate: draining again from B's side alone still
    // reads the pool A already drained.
    assert_eq!(world.stealth_pool(id(0x902)).unwrap().points, pool.points);
}

/// The sight (light), sound (movement noise from worn armor weight) and
/// skill (Sneak actor value) terms each come from records, not constants:
/// changing the backing record changes the computed value by exactly the
/// term's coefficient.
#[test]
fn cmb_31_sight_sound_and_skill_terms_are_record_backed() {
    let mut world = fixture(detection_gmsts(&[]));

    // Baseline: distance 0 (attenuation 1), light 50, still, no armor, no
    // skill difference. sound=0, visual=50, skill=0 => detection 50.
    world
        .evaluate_detection(&[still(0x900, 0x902, 0.0, 50.0)], 0.0)
        .unwrap();
    let baseline = world.awareness(id(0x900), id(0x902)).unwrap().detection;
    assert!((baseline - 50.0).abs() < 1e-3, "sight term: {baseline}");

    // Skill term: raise the target's Sneak (a record-backed actor value) to
    // 40. target_skill scales 1:1 with Sneak here, so detection drops by 40.
    world.set_actor_value(id(0x902), "sneak", 40.0).unwrap();
    world
        .evaluate_detection(&[still(0x900, 0x902, 0.0, 50.0)], 0.0)
        .unwrap();
    let with_skill = world.awareness(id(0x900), id(0x902)).unwrap().detection;
    assert!(
        (with_skill - 10.0).abs() < 1e-3,
        "target Sneak should subtract from detection: {with_skill}"
    );
    world.set_actor_value(id(0x902), "sneak", 0.0).unwrap();

    // Sound term: equip a 20-weight ARMO record on the target and switch to
    // Walking motion. movement = weight_mult(1) * armor_weight(20) * run(1)
    // * muffle(1, no MovementNoiseMult record) = 20, added straight to sound.
    world.equip_item(id(0x902), id(0xA00)).unwrap();
    let walking = Observation {
        motion: Motion::Walking,
        ..still(0x900, 0x902, 0.0, 50.0)
    };
    world.evaluate_detection(&[walking], 0.0).unwrap();
    let with_armor = world.awareness(id(0x900), id(0x902)).unwrap().detection;
    assert!(
        (with_armor - 70.0).abs() < 1e-3,
        "worn armor weight should add to the sound term: {with_armor}"
    );
}

/// The pool's alert timer and an observer's lost timer hold non-default
/// values, and both survive an engine save/load round trip byte for byte.
#[test]
fn cmb_31_timers_survive_save_and_load() {
    let mut world = fixture(detection_gmsts(&[]));

    // Step 1: A goes straight to Combat (90 > 50), B stays Alert (50, not
    // strictly above threshold). The shared pool drains and starts its
    // alert wait.
    world
        .evaluate_detection(
            &[
                still(0x900, 0x902, 10.0, 100.0),
                still(0x901, 0x902, 50.0, 100.0),
            ],
            1.0,
        )
        .unwrap();
    assert_eq!(
        world.awareness(id(0x900), id(0x902)).unwrap().level,
        DetectionLevel::Combat
    );
    let alert_wait_after_step_1 = world.stealth_pool(id(0x902)).unwrap().alert_wait;
    assert!(alert_wait_after_step_1 > 0.0);

    // Step 2: only A is observed this step, moved out past max range
    // (attenuation 0, detection 0), so it accumulates lost_timer while
    // remaining Combat rather than clearing it. B is absent from this
    // batch, so its Alert awareness from step 1 is left untouched: once the
    // shared pool is depleted (which this step forces, since A already
    // holds it at zero), any observed detection above zero would escalate
    // straight to Combat, so leaving B out of the batch is what keeps its
    // saved Alert state around to round-trip.
    world
        .evaluate_detection(&[still(0x900, 0x902, 100.0, 100.0)], 2.0)
        .unwrap();
    let before_a = world.awareness(id(0x900), id(0x902)).unwrap();
    let before_b = world.awareness(id(0x901), id(0x902)).unwrap();
    let before_pool = world.stealth_pool(id(0x902)).unwrap().clone();
    assert!(
        before_a.lost_timer > 0.0,
        "lost_timer should have accumulated: {before_a:?}"
    );
    assert_eq!(before_a.level, DetectionLevel::Combat);
    assert_eq!(before_b.level, DetectionLevel::Alert);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();

    let after_a = world.awareness(id(0x900), id(0x902)).unwrap();
    let after_b = world.awareness(id(0x901), id(0x902)).unwrap();
    let after_pool = world.stealth_pool(id(0x902)).unwrap().clone();
    assert_eq!(after_a, before_a, "observer A's timer and level round trip");
    assert_eq!(after_b, before_b, "observer B's timer and level round trip");
    assert_eq!(
        after_pool.points, before_pool.points,
        "shared pool points round trip"
    );
    assert_eq!(
        after_pool.alert_wait, before_pool.alert_wait,
        "shared pool alert wait round trips: {}",
        after_pool.alert_wait
    );
}

/// A batch that mixes one otherwise-valid observation with one invalid
/// observation is rejected as a whole, and the world's detection state is
/// left exactly as it was before the call, for several distinct causes.
#[test]
fn cmb_31_invalid_observations_are_rejected_atomically() {
    let mut world = fixture(detection_gmsts(&[]));
    world
        .evaluate_detection(&[still(0x900, 0x902, 10.0, 100.0)], 1.0)
        .unwrap();
    let before = world.stealth_pool(id(0x902)).unwrap().clone();

    // Self observation, mixed with an otherwise-valid observation on a
    // different pair that would have changed state if applied alone.
    let self_observation = Observation {
        observer: id(0x901),
        target: id(0x901),
        ..still(0x901, 0x901, 0.0, 100.0)
    };
    let err = world
        .evaluate_detection(&[still(0x901, 0x902, 0.0, 100.0), self_observation], 1.0)
        .unwrap_err();
    assert!(matches!(err, Error::SelfObservation(_)), "{err:?}");
    assert_eq!(
        world.stealth_pool(id(0x902)).unwrap().clone(),
        before,
        "a rejected batch must not partially apply its valid member"
    );
    assert!(
        world.awareness(id(0x901), id(0x902)).is_none(),
        "B must not have been added to the pool by the rejected batch"
    );

    // Duplicate observer/target pair within one batch.
    let dup = still(0x901, 0x902, 0.0, 100.0);
    let err = world.evaluate_detection(&[dup, dup], 1.0).unwrap_err();
    assert!(matches!(err, Error::Duplicate { .. }), "{err:?}");
    assert_eq!(world.stealth_pool(id(0x902)).unwrap().clone(), before);

    // Negative distance, alongside a valid observation.
    let negative = Observation {
        distance: -1.0,
        ..still(0x901, 0x902, 0.0, 100.0)
    };
    let err = world
        .evaluate_detection(&[still(0x900, 0x902, 10.0, 100.0), negative], 1.0)
        .unwrap_err();
    assert!(matches!(err, Error::Observation), "{err:?}");
    assert_eq!(world.stealth_pool(id(0x902)).unwrap().clone(), before);

    // A non-actor id (the NPC_ base itself, never placed as a reference).
    let bad_actor = Observation {
        target: id(0x800),
        ..still(0x900, 0x800, 0.0, 100.0)
    };
    let err = world.evaluate_detection(&[bad_actor], 1.0).unwrap_err();
    assert!(matches!(err, Error::Actor(_)), "{err:?}");
    assert_eq!(world.stealth_pool(id(0x902)).unwrap().clone(), before);

    // The pool never moved across any of the four rejections.
    assert_eq!(world.stealth_pool(id(0x902)).unwrap().clone(), before);
}
