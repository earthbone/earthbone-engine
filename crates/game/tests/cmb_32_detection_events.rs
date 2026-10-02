// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-32 sound events retain their actor owner and source cell.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_game::detection::{DetectionSettings, Motion, Observation, Senses, detection_value};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"DATA", [1_u8, 0]));
    plugin.push_top(Record::new(Tag(*b"STAT"), 2));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 4));
    let mut source = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    source.placement.cell = Some(1);
    plugin.push_top(source);
    let mut owner = Record::new(Tag(*b"ACHR"), 5).sub(b"NAME", 4_u32.to_le_bytes());
    owner.placement.cell = Some(1);
    plugin.push_top(owner);
    let mut observer = Record::new(Tag(*b"ACHR"), 6).sub(b"NAME", 4_u32.to_le_bytes());
    observer.placement.cell = Some(1);
    plugin.push_top(observer);
    let mut order = LoadOrder::new();
    order.add("Sound.esm", plugin).unwrap();
    World::new(order)
}

fn settings(world: &mut World) {
    let names = [
        "fSneakBaseValue",
        "fSneakMaxDistance",
        "fSneakDistanceAttenuationExponent",
        "fSneakSoundsMult",
        "fSneakSoundLosMult",
        "fSneakEquippedWeightBase",
        "fSneakEquippedWeightMult",
        "fSneakRunningMult",
        "fSneakActionMult",
        "fSneakLightMoveMult",
        "fSneakLightRunMult",
        "fDetectionSneakLightMod",
        "fSneakLightMult",
        "fSneakStealthBoyMult",
        "fSneakPerceptionSkillMin",
        "fSneakPerceptionSkillMax",
        "fSneakAlertMod",
        "fSneakCombatMod",
        "fCombatStealthPointMax",
        "fCombatStealthPointDrainMult",
        "fCombatStealthPointRegenMult",
        "fCombatStealthPointRegenMin",
        "fCombatStealthPointRegenAlertWaitTime",
        "fCombatStealthPointRegenLostWaitTime",
        "iCombatStealthPointDetectionThreshold",
        "iCombatStealthPointSneakDetectionThreshold",
    ];
    let mut values = [0.0; 26];
    values[1] = 100.0;
    values[2] = 1.0;
    values[3] = 1.0;
    values[4] = 1.0;
    values[8] = 1.0;
    values[12] = 1.0;
    values[18] = 100.0;
    values[19] = 1.0;
    for (name, value) in names.into_iter().zip(values) {
        world
            .settings
            .insert(name.to_ascii_lowercase(), Value::Float(value));
    }
}

#[test]
fn cmb_32_emitted_sound_uses_owner_identity_and_source_cell() {
    let mut world = world();
    assert!(world.create_detection_event(id(3), id(5), 125));
    let events = world.take_detection_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].owner, events[0].cell, events[0].sound_level),
        (id(5), id(1), 125.0)
    );
    assert!(
        world.take_detection_events().is_empty(),
        "event fires for one observation step"
    );
}

#[test]
fn cmb_32_invalid_sound_does_not_change_event_queue() {
    let mut world = world();
    assert!(!world.create_detection_event(id(3), id(2), 80));
    assert!(!world.create_detection_event(id(3), id(5), -1));
    assert!(
        world.take_detection_events().is_empty(),
        "invalid event is ignored"
    );
}

#[test]
fn cmb_32_loading_state_discards_unconsumed_sound() {
    let mut world = world();
    assert!(world.create_detection_event(id(3), id(5), 70));
    let bytes = world.save_state();
    world.load_state(&bytes).unwrap();
    assert!(
        world.take_detection_events().is_empty(),
        "one-step sound does not survive a load"
    );
}

#[test]
fn cmb_32_sound_uses_emitter_distance_with_separate_owner() {
    let mut values = [0.0; 26];
    values[1] = 100.0;
    values[2] = 1.0;
    values[3] = 1.0;
    values[4] = 1.0;
    values[8] = 1.0;
    values[18] = 100.0;
    let settings = DetectionSettings::from_values(values).unwrap();
    let senses = Senses {
        observer_sneak: 0.0,
        target_sneak: 0.0,
        armor_weight: 0.0,
        muffle: 1.0,
        blindness: 0.0,
        invisible: false,
        observer_alert: false,
        observer_busy: false,
    };
    let mut observation = Observation {
        observer: id(5),
        target: id(4),
        distance: 100.0,
        line_of_sight: false,
        light_level: 0.0,
        motion: Motion::Still,
        action_sound: 80.0,
        action_sound_distance: Some(0.0),
        action_sound_line_of_sight: Some(true),
    };
    let nearby_event = detection_value(&settings, &observation, &senses).unwrap();
    observation.action_sound_distance = None;
    let distant_actor = detection_value(&settings, &observation, &senses).unwrap();
    assert_eq!((nearby_event, distant_actor), (80.0, 0.0));
}

#[test]
fn cmb_32_live_inputs_change_shared_awareness_without_changing_identity() {
    let mut world = world();
    settings(&mut world);
    let mut observation = Observation {
        observer: id(6),
        target: id(5),
        distance: 10.0,
        line_of_sight: false,
        light_level: 0.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    };
    world.evaluate_detection(&[observation], 0.0).unwrap();
    let hidden = world.awareness(id(6), id(5)).unwrap().detection;
    observation.line_of_sight = true;
    observation.light_level = 80.0;
    world.evaluate_detection(&[observation], 0.0).unwrap();
    let lit = world.awareness(id(6), id(5)).unwrap().detection;
    observation.line_of_sight = false;
    observation.light_level = 0.0;
    observation.action_sound = 100.0;
    world.evaluate_detection(&[observation], 0.0).unwrap();
    let heard = world.awareness(id(6), id(5)).unwrap().detection;
    assert!(
        lit > hidden && heard > hidden,
        "light and sound increase shared detection"
    );
    assert!(
        world
            .stealth_pool(id(5))
            .unwrap()
            .observers
            .contains_key(&id(6))
    );
}
