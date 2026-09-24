// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-31, shared actor detection and stealth
//! points (`World::evaluate_detection`, `crate::detection::StealthPool::step`).
//!
//! Sources: `docs/combat-ai.md`, section "Shared stealth points (CMB-31)",
//! and the CK Stealth Points page it summarizes. Every assertion states the
//! contract those sources describe, not whatever the current code returns.
//!
//! Fixtures build a minimal load order with one `NPC_` base and several
//! `ACHR` references, and inject every detection GMST through
//! `World::settings` (the same pattern `leveled_items.rs` uses), so no
//! `Skyrim.esm` or real plugin is required.

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionLevel, Error as DetectionError, Motion, Observation};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

/// One `NPC_` base at 0x800, and one `ACHR` reference for every object id
/// passed in, all pointing at that base through `NAME`.
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

/// Every detection GMST, set so the formula collapses to
/// `value = action_sound` at distance zero: skill terms are zeroed by equal
/// perception bounds, visual is zeroed by `fSneakLightMult`, movement is
/// zeroed by `Motion::Still`, and both line-of-sight multipliers are 1.0 so
/// `line_of_sight` does not matter either. `overrides` replaces named
/// entries for tests that need a specific threshold or drain rate.
fn set_all_settings(world: &mut World, overrides: &[(&str, f32)]) {
    let mut values: BTreeMap<&str, f32> = [
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
    for (name, value) in overrides {
        values.insert(name, *value);
    }
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

/// docs/combat-ai.md: "Combat holds the pool at zero." The code's own
/// comment in `StealthPool::step` repeats it: "Stealth Points always = 0
/// when enemies are in combat with you." Both describe a state, not a
/// one-step-late transition: the pool must already read zero on the same
/// step an observer's detection first crosses the combat threshold, not
/// starting the step after.
#[test]
fn combat_threshold_crossing_zeros_the_shared_pool_on_the_same_step() {
    let mut world = build_world(&[0x900, 0x901]);
    // Drain is disabled so a pass without the fix cannot zero the pool by
    // ordinary drain math and mask the missing same-step "held" check.
    set_all_settings(&mut world, &[("fCombatStealthPointDrainMult", 0.0)]);
    let observer = id(0x900);
    let target = id(0x901);

    world
        .evaluate_detection(&[observation(observer, target, 1000.0)], 1.0)
        .unwrap();

    let awareness = world
        .awareness(observer, target)
        .expect("the observer must be recorded against the target's pool");
    assert_eq!(
        awareness.level,
        DetectionLevel::Combat,
        "a detection value of 1000 must already exceed the combat threshold of 50 on first contact"
    );
    let pool = world
        .stealth_pool(target)
        .expect("evaluating an observation must create the target's pool");
    assert_eq!(
        pool.points, 0.0,
        "docs/combat-ai.md 'Shared stealth points (CMB-31)': Combat holds the pool at zero, \
         on the step Combat starts, not the step after"
    );
}

/// docs/combat-ai.md: "One pool exists per target, shared by every observer
/// of that target... The pool drains at `fCombatStealthPointDrainMult` times
/// the highest positive detection." Two observers of one target must share
/// one pool driven by the higher of the two values, not the sum and not
/// either one alone.
#[test]
fn two_observers_share_one_pool_driven_by_the_higher_detection_value() {
    let mut world = build_world(&[0x900, 0x901, 0x902]);
    // Threshold is pushed far out of reach so neither observer enters
    // Combat and the drain math can be checked in isolation.
    set_all_settings(
        &mut world,
        &[
            ("iCombatStealthPointDetectionThreshold", 100_000.0),
            ("iCombatStealthPointSneakDetectionThreshold", 100_000.0),
        ],
    );
    let low = id(0x900);
    let high = id(0x901);
    let target = id(0x902);

    world
        .evaluate_detection(
            &[
                observation(low, target, 2.0),
                observation(high, target, 5.0),
            ],
            1.0,
        )
        .unwrap();

    let pool = world.stealth_pool(target).expect("pool must exist");
    assert_eq!(
        pool.points, 95.0,
        "shared pool must drain by the higher observer's value (5.0), not the sum (7.0) \
         or the lower observer's value (2.0): 100.0 - 5.0 * 1.0 * 1.0 = 95.0"
    );
    assert_eq!(
        world
            .awareness(low, target)
            .expect("low observer recorded")
            .detection,
        2.0,
        "each observer keeps its own detection value even while sharing one pool"
    );
    assert_eq!(
        world
            .awareness(high, target)
            .expect("high observer recorded")
            .detection,
        5.0
    );
}

/// docs/combat-ai.md: "The batch is validated before anything changes...
/// a repeated pair... rejects the whole batch and leaves every pool as it
/// was." A duplicate pair placed after other, individually valid entries
/// must still reject the whole step, and must leave no pool behind for the
/// entries that preceded it.
#[test]
fn a_duplicate_pair_later_in_the_batch_rejects_the_whole_step() {
    let mut world = build_world(&[0x900, 0x901, 0x902]);
    set_all_settings(&mut world, &[]);
    let a = id(0x900);
    let b = id(0x901);
    let target = id(0x902);

    let batch = [
        observation(a, target, 1.0),
        observation(b, target, 1.0),
        observation(a, target, 1.0), // duplicate of the first pair
    ];
    let err = world.evaluate_detection(&batch, 1.0).unwrap_err();
    assert!(
        matches!(err, DetectionError::Duplicate { observer, target: t } if observer == a && t == target),
        "expected Error::Duplicate for the repeated (observer, target) pair, got {err:?}"
    );
    assert!(
        world.stealth_pool(target).is_none(),
        "docs/combat-ai.md: a rejected batch 'leaves every pool as it was'; no pool may be \
         created for the two individually valid observations that preceded the duplicate"
    );
}

/// docs/combat-ai.md: "The batch is validated before anything changes. A
/// non-actor... rejects the whole batch." A target that is not a live
/// `ACHR` reference, and one with no record at all, must both be rejected
/// as invalid observations.
#[test]
fn non_actor_and_unresolvable_targets_are_rejected() {
    let mut world = build_world(&[0x900]);
    set_all_settings(&mut world, &[]);
    let observer = id(0x900);
    let base_record = id(0x800); // the NPC_ base, not an ACHR reference
    let ghost = id(0x999); // no record at all in this load order

    for target in [base_record, ghost] {
        let err = world
            .evaluate_detection(&[observation(observer, target, 1.0)], 1.0)
            .unwrap_err();
        assert!(
            matches!(err, DetectionError::Actor(t) if t == target),
            "expected Error::Actor for a target that is not a live actor ({target:?}), got {err:?}"
        );
        assert!(
            world.stealth_pool(target).is_none(),
            "a rejected observation must not create a pool for its target"
        );
    }
}
