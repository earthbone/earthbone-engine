// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-31's save/load contract.
//!
//! Sources: the done check ("timers that survive save/load") and
//! `crates/game/src/world/detection.rs`, whose doc comments state the SOEW
//! version 29 block is written when a pool exists and "An absent block is
//! an empty state". `docs/combat-ai.md`: "The pools and each observer's
//! state and timers are written in SOEW version 29 and read back with the
//! same checks."
//!
//! Both tests drive the shared pool through `World::evaluate_detection`
//! with every detection GMST pinned by `World::settings`, so the resulting
//! points, alert wait and per-observer level, detection value and lost
//! timer are exact, reproducible numbers rather than opaque state.

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{Awareness, DetectionLevel, Motion, Observation};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn build_world(actor_objects: &[u32]) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for &object in actor_objects {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x0800_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Detection.esp", plugin).unwrap();
    World::new(order)
}

/// Same collapse as `cmb_31_shared_detection_adversarial.rs`: at distance
/// zero, with skill and visual terms zeroed and both line-of-sight
/// multipliers at 1.0, `value = action_sound` exactly.
fn set_all_settings(world: &mut World) {
    let values: BTreeMap<&str, f32> = [
        ("fSneakBaseValue", 0.0),
        ("fSneakMaxDistance", 2500.0),
        ("fSneakDistanceAttenuationExponent", 2.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 12.0),
        ("fSneakEquippedWeightMult", 0.5),
        ("fSneakRunningMult", 2.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.01),
        ("fSneakLightRunMult", 0.2),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 0.0),
        ("fSneakStealthBoyMult", 0.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 0.0),
        ("fSneakAlertMod", 1.0),
        ("fSneakCombatMod", -0.4),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 20.0),
        ("fCombatStealthPointRegenLostWaitTime", 100.0),
        ("iCombatStealthPointDetectionThreshold", 50.0),
        ("iCombatStealthPointSneakDetectionThreshold", 25.0),
    ]
    .into_iter()
    .collect();
    for (name, value) in values {
        world
            .settings
            .insert(name.to_ascii_lowercase(), soe_papyrus::Value::Float(value));
    }
}

fn observation(observer: GlobalFormId, target: GlobalFormId, action_sound: f32) -> Observation {
    Observation {
        observer,
        target,
        distance: 0.0,
        line_of_sight: false,
        light_level: 0.0,
        motion: Motion::Still,
        action_sound,
    }
}

/// Drives one target's pool through two steps so it carries state that is
/// not the type's default in every field: `points` at zero, a nonzero
/// `alert_wait`, one observer stuck in Combat with a nonzero `lost_timer`,
/// and a second observer holding at Alert.
///
/// Step 1: both observers exceed zero, `observer` (60.0) crosses the
/// Combat threshold (50.0) and `witness` (20.0) does not, so `witness`
/// becomes Alert. The pool drains from full (100.0) by the higher value:
/// 100.0 - 60.0 * 1.0 * 1.0 = 40.0, still above zero so the pool is not
/// depleted and `witness` is not swept into Combat by depletion. `alert_wait`
/// is set to `fCombatStealthPointRegenAlertWaitTime` (20.0).
/// Step 2 (elapsed 3.0): both observers drop to 0.0. `observer` is already
/// Combat, so the pool is held at zero and `lost_timer` accumulates to
/// 3.0, short of `fCombatStealthPointRegenLostWaitTime` (100.0). `witness`
/// stays Alert, since a return to Normal needs `alert_wait` to be
/// exhausted, which the held branch does not touch.
fn build_detection_state(
    world: &mut World,
    observer: GlobalFormId,
    witness: GlobalFormId,
    target: GlobalFormId,
) {
    world
        .evaluate_detection(
            &[
                observation(observer, target, 60.0),
                observation(witness, target, 20.0),
            ],
            1.0,
        )
        .unwrap();
    world
        .evaluate_detection(
            &[
                observation(observer, target, 0.0),
                observation(witness, target, 0.0),
            ],
            3.0,
        )
        .unwrap();
}

/// The done check: "timers that survive save/load". Loading a save must
/// reproduce the pool and every observer's level, detection value and
/// timer exactly, not merely leave whatever was already in memory
/// untouched. Loading into a second, freshly built `World` (never passed
/// through `evaluate_detection` itself) is the only way to tell "restored
/// from the save" apart from "coincidentally already correct".
#[test]
fn load_state_restores_saved_stealth_pools_and_observer_timers() {
    let mut source = build_world(&[0x900, 0x901, 0x902]);
    set_all_settings(&mut source);
    let observer = id(0x900);
    let witness = id(0x901);
    let target = id(0x902);
    build_detection_state(&mut source, observer, witness, target);

    let expected_pool_points = 0.0;
    let expected_alert_wait = 20.0;
    let expected_observer = Awareness {
        level: DetectionLevel::Combat,
        detection: 0.0,
        lost_timer: 3.0,
    };
    let expected_witness = Awareness {
        level: DetectionLevel::Alert,
        detection: 0.0,
        lost_timer: 0.0,
    };
    // Sanity: the state built above is exactly what the loaded copy will be
    // compared against, not an assumption about the source's internals.
    let source_pool = source.stealth_pool(target).expect("source pool exists");
    assert_eq!(source_pool.points, expected_pool_points);
    assert_eq!(source_pool.alert_wait, expected_alert_wait);
    assert_eq!(source.awareness(observer, target), Some(expected_observer));
    assert_eq!(source.awareness(witness, target), Some(expected_witness));

    let saved = source.save_state();

    let mut loaded = build_world(&[0x900, 0x901, 0x902]);
    loaded.load_state(&saved).unwrap();

    let pool = loaded.stealth_pool(target).expect(
        "crates/game/src/world/save.rs: load_state must restore the version 29 detection \
         block into self.detection (docs/combat-ai.md: 'read back with the same checks'); \
         a freshly built world has no pool until the save is applied",
    );
    assert_eq!(
        pool.points, expected_pool_points,
        "loaded pool points must match what was saved"
    );
    assert_eq!(
        pool.alert_wait, expected_alert_wait,
        "loaded pool alert_wait must match what was saved"
    );
    assert_eq!(
        loaded.awareness(observer, target),
        Some(expected_observer),
        "the Combat observer's level, detection value and lost_timer must survive the round trip"
    );
    assert_eq!(
        loaded.awareness(witness, target),
        Some(expected_witness),
        "the Alert observer's level, detection value and lost_timer must survive the round trip"
    );
}

/// `crates/game/src/world/detection.rs`: "An absent block is an empty
/// state". A save taken before any detection ever ran carries no block at
/// all (`write_detection` returns early while `pools` is empty), the same
/// byte layout an older, pre-CMB-31 save has. Loading that save onto a
/// world that currently holds pools from a later point in the same session
/// must clear them, not leave the newer in-memory state behind as if the
/// load had not happened.
#[test]
fn loading_a_save_with_no_detection_data_clears_existing_pools() {
    let mut world = build_world(&[0x900, 0x901, 0x902]);
    set_all_settings(&mut world);
    let observer = id(0x900);
    let witness = id(0x901);
    let target = id(0x902);

    let empty_save = world.save_state();
    assert!(
        world.stealth_pool(target).is_none(),
        "no pool should exist before any observation has been evaluated"
    );

    build_detection_state(&mut world, observer, witness, target);
    assert!(
        world.stealth_pool(target).is_some(),
        "the pool must exist once evaluate_detection has run"
    );

    world.load_state(&empty_save).unwrap();
    assert!(
        world.stealth_pool(target).is_none(),
        "loading a save with no detection block must clear pools built after that save was \
         taken, the same way every other restored field replaces its in-memory value; a stale \
         Combat pool surviving a load onto an older save is a save/load continuation bug, \
         done check: 'timers that survive save/load' implies load actually applies them"
    );
    assert!(
        world.awareness(observer, target).is_none(),
        "a cleared pool must not still report the observer's Combat awareness"
    );
}
