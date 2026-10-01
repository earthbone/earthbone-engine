// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionEvent, Motion};
use soe_game::world::ContactActor;
use soe_game::World;
use soe_papyrus::Value;

use soe_godot_host::detection::{light_level, observations};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn actor(object: u32, cell: GlobalFormId, position: [f32; 3]) -> ContactActor {
    ContactActor {
        actor: id(object),
        cell,
        position,
        rotation: [0.0; 3],
        scale: 1.0,
    }
}

fn light_world(ambient: [u8; 3], flags: u32, radius: u32, rgb: [u8; 3]) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", ambient));
    let mut data = Vec::new();
    data.extend_from_slice(&(-1_i32).to_le_bytes());
    data.extend_from_slice(&radius.to_le_bytes());
    data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0]);
    data.extend_from_slice(&flags.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("DetectionLight.esm", plugin).unwrap();
    World::new(order)
}

fn add_settings(world: &mut World) {
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

fn actor_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1));
    plugin.push_top(Record::new(Tag(*b"CELL"), 2));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 3));
    for (object, cell) in [(4, 1), (5, 1)] {
        let mut reference = Record::new(Tag(*b"ACHR"), object)
            .sub(b"NAME", 3_u32.to_le_bytes());
        reference.placement.cell = Some(cell);
        plugin.push_top(reference);
    }
    let mut order = LoadOrder::new();
    order.add("DetectionActors.esm", plugin).unwrap();
    let mut world = World::new(order);
    add_settings(&mut world);
    world
}

#[test]
fn cmb_32_light_level_obeys_ck_range_with_bright_ambient_and_light() {
    let world = light_world([255; 3], 0, 100, [255; 3]);
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 150.0);
}

#[test]
fn cmb_32_negative_light_reduces_ambient_only_inside_its_radius() {
    let world = light_world([120; 3], 1 << 2, 100, [255; 3]);
    let inside = light_level(&world, id(1), [0.0; 3]);
    let outside = light_level(&world, id(1), [100.0, 0.0, 0.0]);
    assert!(inside < outside, "negative light reduces local light");
    assert_eq!(outside, 120.0 * (100.0 / 255.0));
}

#[test]
fn cmb_32_sound_is_local_to_its_cell_and_does_not_grant_sight() {
    let first = id(1);
    let second = id(2);
    let observer = actor(4, first, [0.0; 3]);
    let target = actor(5, first, [20.0, 0.0, 0.0]);
    let event = DetectionEvent {
        owner: target.actor,
        cell: second,
        position: [1.0, 0.0, 0.0],
        sound_level: 100.0,
    };
    let values = observations(
        &[observer, target],
        &[event],
        &BTreeMap::new(),
        |_, _| false,
        |_, _| 0.0,
    );
    let pair = values
        .iter()
        .find(|value| value.observer == observer.actor && value.target == target.actor)
        .unwrap();
    assert_eq!(pair.action_sound, 0.0);
    assert!(!pair.line_of_sight);
}

#[test]
fn cmb_32_cell_change_and_engine_save_keep_awareness_on_actor_form_ids() {
    let mut world = actor_world();
    let first_cell = id(1);
    let second_cell = id(2);
    let before_observations = observations(
        &[
            actor(4, first_cell, [0.0; 3]),
            actor(5, first_cell, [10.0, 0.0, 0.0]),
        ],
        &[],
        &BTreeMap::new(),
        |_, _| true,
        |_, _| 100.0,
    );
    let before_pair = before_observations
        .iter()
        .find(|value| value.observer == id(4) && value.target == id(5))
        .unwrap();
    world.evaluate_detection(&[*before_pair], 0.0).unwrap();
    let before = world.awareness(id(4), id(5)).unwrap();
    let bytes = world.save_state();
    world.load_state(&bytes).unwrap();
    let event = DetectionEvent {
        owner: id(5),
        cell: second_cell,
        position: [12.0, 0.0, 0.0],
        sound_level: 100.0,
    };
    let after_observations = observations(
        &[
            actor(4, second_cell, [0.0; 3]),
            actor(5, second_cell, [10.0, 0.0, 0.0]),
        ],
        &[event],
        &BTreeMap::new(),
        |_, _| false,
        |_, _| 0.0,
    );
    let after_pair = after_observations
        .iter()
        .find(|value| value.observer == id(4) && value.target == id(5))
        .unwrap();
    assert!(!after_pair.line_of_sight);
    assert_eq!(after_pair.action_sound, 100.0);
    world.evaluate_detection(&[*after_pair], 0.0).unwrap();
    let after = world.awareness(id(4), id(5)).unwrap();
    assert_ne!(after.detection, before.detection);
    assert_eq!(world.stealth_pool(id(5)).unwrap().observers.len(), 1);
}
