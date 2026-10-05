// SPDX-License-Identifier: GPL-3.0-or-later

//! Live detection samples from resident FormIDs. Godot supplies sight rays,
//! while CELL/XCLL and placed LIGH records supply detection light. The light
//! model follows xEdit's field layouts and RAID's distinction between engine
//! detection light and final screen brightness.

use std::collections::BTreeMap;

use soe_esm::{GlobalFormId, Tag};
use soe_game::detection::{DetectionEvent, Motion, Observation};
use soe_game::world::ContactActor;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[path = "detection_lights.rs"]
mod lights;
#[path = "detection_spatial.rs"]
mod spatial;
pub use lights::{light_level, LightSamples};
pub use spatial::observations_in_range;

#[cfg(test)]
use lights::full_scan_light_level;
#[cfg(test)]
#[path = "prf_31_detection_tests.rs"]
mod prf_31_tests;

/// Make one observation for each resident ordered actor pair in one cell.
/// Geometry and light are sampled afresh, so changing cells replaces only the
/// host inputs. Detection history remains keyed by the actors' FormIDs.
pub fn observations(
    actors: &[ContactActor],
    events: &[DetectionEvent],
    motions: &BTreeMap<GlobalFormId, Motion>,
    visible: impl FnMut(&ContactActor, &ContactActor) -> bool,
    light: impl FnMut(GlobalFormId, [f32; 3]) -> f32,
) -> Vec<Observation> {
    observations_in_range(actors, events, motions, f32::MAX, visible, light)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1]).hypot(v[2])
}

#[cfg(test)]
mod cmb_32_tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};
    use soe_game::World;

    fn actor(file: u16, object: u32, cell: GlobalFormId) -> ContactActor {
        ContactActor {
            actor: GlobalFormId { file, object },
            cell,
            position: [object as f32, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: 1.0,
        }
    }

    #[test]
    fn cmb_32_occlusion_light_and_sound_change_observations() {
        let cell = GlobalFormId { file: 0, object: 1 };
        let a = actor(0, 2, cell);
        let b = actor(0, 3, cell);
        let first = observations(&[a, b], &[], &BTreeMap::new(), |_, _| false, |_, _| 0.0);
        let event = DetectionEvent {
            owner: b.actor,
            cell,
            position: b.position,
            sound_level: 100.0,
        };
        let second = observations(
            &[a, b],
            &[event],
            &BTreeMap::new(),
            |_, _| true,
            |_, _| 90.0,
        );
        assert!(!first[0].line_of_sight && second[0].line_of_sight);
        assert_eq!((first[0].light_level, second[0].light_level), (0.0, 90.0));
        assert_eq!(
            (first[0].action_sound, second[0].action_sound),
            (0.0, 100.0)
        );
    }

    #[test]
    fn cmb_32_cell_change_retains_actor_form_identity() {
        let first = GlobalFormId { file: 0, object: 1 };
        let second = GlobalFormId { file: 0, object: 4 };
        let a = actor(0, 2, first);
        let b = actor(0, 3, first);
        let before = observations(&[a, b], &[], &BTreeMap::new(), |_, _| true, |_, _| 10.0);
        let after = observations(
            &[
                ContactActor { cell: second, ..a },
                ContactActor { cell: second, ..b },
            ],
            &[],
            &BTreeMap::new(),
            |_, _| false,
            |_, _| 20.0,
        );
        assert_eq!(
            (before[0].observer, before[0].target),
            (after[0].observer, after[0].target)
        );
        assert!(!after[0].line_of_sight);
    }

    #[test]
    fn cmb_32_sound_source_can_have_an_owner_in_another_cell() {
        let cell = GlobalFormId { file: 0, object: 1 };
        let other = GlobalFormId { file: 0, object: 4 };
        let observer = actor(0, 2, cell);
        let owner = actor(0, 3, other);
        let event = DetectionEvent {
            owner: owner.actor,
            cell,
            position: observer.position,
            sound_level: 125.0,
        };
        let sampled = observations(
            &[observer, owner],
            &[event],
            &BTreeMap::new(),
            |_, _| true,
            |_, _| 0.0,
        );
        assert_eq!(
            (sampled[0].observer, sampled[0].target),
            (observer.actor, owner.actor)
        );
        assert_eq!(sampled[0].action_sound_distance, Some(0.0));
        assert!(
            !sampled[0].line_of_sight,
            "an event does not grant sight to its owner"
        );
    }

    #[test]
    fn cmb_32_cell_ambient_and_placed_light_follow_records() {
        let mut plugin = Plugin::new(FileHeader::default());
        let cell = GlobalFormId { file: 0, object: 1 };
        let mut cell_light = vec![0_u8; 92];
        cell_light[..3].copy_from_slice(&[60, 60, 60]);
        plugin.push_top(Record::new(T(b"CELL"), 1).sub(b"XCLL", cell_light));
        let mut data = Vec::new();
        data.extend_from_slice(&(-1_i32).to_le_bytes());
        data.extend_from_slice(&100_u32.to_le_bytes());
        data.extend_from_slice(&[255, 255, 255, 0]);
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.resize(48, 0);
        plugin.push_top(Record::new(T(b"LIGH"), 2).sub(b"DATA", data));
        let mut reference = Record::new(T(b"REFR"), 3)
            .sub(b"NAME", 2_u32.to_le_bytes())
            .sub(b"DATA", [0_u8; 24]);
        reference.placement.cell = Some(1);
        plugin.push_top(reference);
        let mut order = LoadOrder::new();
        order.add("Light.esm", plugin).unwrap();
        let world = World::new(order);
        let near = light_level(&world, cell, [0.0; 3]);
        let far = light_level(&world, cell, [200.0, 0.0, 0.0]);
        assert!(near > far, "placed light raises nearby detection light");
        assert!(far > 0.0, "cell ambient remains beyond the light radius");
    }

    #[test]
    fn cmb_32_cell_inherits_ambient_from_lighting_template() {
        let mut plugin = Plugin::new(FileHeader::default());
        let mut cell_light = vec![0_u8; 92];
        cell_light[88] = 1;
        plugin.push_top(
            Record::new(T(b"CELL"), 1)
                .sub(b"XCLL", cell_light)
                .sub(b"LTMP", 2_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(T(b"LGTM"), 2).sub(b"DATA", [120; 24]));
        let mut order = LoadOrder::new();
        order.add("Template.esm", plugin).unwrap();
        let world = World::new(order);
        let cell = GlobalFormId { file: 0, object: 1 };
        assert!(light_level(&world, cell, [0.0; 3]) > 40.0);
    }

    #[test]
    fn cmb_32_vanilla_light_records_are_read_when_installed() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
        let order = source.shared_order();
        let cell = source.first_populated_interior_cell(10).unwrap();
        let world = World::from_shared_order(order);
        let value = light_level(&world, cell, [0.0; 3]);
        assert!(
            (0.0..=150.0).contains(&value),
            "vanilla light sample is bounded"
        );
    }
}
