// SPDX-License-Identifier: GPL-3.0-or-later

//! Live detection samples from resident FormIDs. Godot supplies sight rays,
//! while CELL/XCLL and placed LIGH records supply detection light. The light
//! model follows xEdit's field layouts and RAID's distinction between engine
//! detection light and final screen brightness.

use std::collections::BTreeMap;

use soe_esm::{plugin::flags, GlobalFormId, Tag};
use soe_game::detection::{DetectionEvent, Motion, Observation};
use soe_game::world::ContactActor;
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// Make one observation for each resident ordered actor pair in one cell.
/// Geometry and light are sampled afresh, so changing cells replaces only the
/// host inputs. Detection history remains keyed by the actors' FormIDs.
pub fn observations(
    actors: &[ContactActor],
    events: &[DetectionEvent],
    motions: &BTreeMap<GlobalFormId, Motion>,
    mut visible: impl FnMut(&ContactActor, &ContactActor) -> bool,
    mut light: impl FnMut(GlobalFormId, [f32; 3]) -> f32,
) -> Vec<Observation> {
    let mut out = Vec::new();
    let mut levels = BTreeMap::new();
    for observer in actors {
        for target in actors {
            if observer.actor == target.actor || observer.cell != target.cell {
                continue;
            }
            let distance = length(sub(observer.position, target.position));
            let sound = events
                .iter()
                .filter(|event| event.owner == target.actor && event.cell == observer.cell)
                .max_by(|a, b| a.sound_level.total_cmp(&b.sound_level));
            let light_level = *levels
                .entry(target.actor)
                .or_insert_with(|| light(target.cell, target.position));
            out.push(Observation {
                observer: observer.actor,
                target: target.actor,
                distance,
                line_of_sight: visible(observer, target),
                light_level,
                motion: motions.get(&target.actor).copied().unwrap_or_default(),
                action_sound: sound.map_or(0.0, |event| event.sound_level),
                action_sound_distance: sound
                    .map(|event| length(sub(observer.position, event.position))),
                action_sound_line_of_sight: sound.map(|event| {
                    visible(
                        observer,
                        &ContactActor {
                            position: event.position,
                            ..*target
                        },
                    )
                }),
            });
        }
        for event in events.iter().filter(|event| event.cell == observer.cell) {
            if event.owner == observer.actor
                || out
                    .iter()
                    .any(|o| o.observer == observer.actor && o.target == event.owner)
            {
                continue;
            }
            let source = ContactActor {
                actor: event.owner,
                cell: event.cell,
                position: event.position,
                rotation: [0.0; 3],
                scale: 1.0,
            };
            out.push(Observation {
                observer: observer.actor,
                target: event.owner,
                distance: length(sub(observer.position, event.position)),
                line_of_sight: false,
                light_level: 0.0,
                motion: Motion::Still,
                action_sound: event.sound_level,
                action_sound_distance: Some(length(sub(observer.position, event.position))),
                action_sound_line_of_sight: Some(visible(observer, &source)),
            });
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1]).hypot(v[2])
}

/// Detection light from a cell's ambient RGB and enabled placed lights.
/// XCLL RGB is the first three bytes, and LIGH/DATA stores radius, RGB and
/// flags at offsets 4, 8 and 12. A missing ambient field contributes zero.
/// This is a record-derived light estimate in the CK's 0 to 150 range.
pub fn light_level(world: &World, cell: GlobalFormId, point: [f32; 3]) -> f32 {
    let cell_record = world.record(cell);
    let cell_light = cell_record.and_then(|record| record.find(T(b"XCLL")));
    let inherited = cell_light
        .filter(|bytes| bytes.len() >= 92)
        .is_some_and(|bytes| {
            u32::from_le_bytes([bytes[88], bytes[89], bytes[90], bytes[91]]) & 1 != 0
        });
    let template = cell_record
        .and_then(|record| record.find(T(b"LTMP")))
        .filter(|bytes| bytes.len() == 4)
        .and_then(|bytes| {
            world.resolve_from(
                cell,
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            )
        })
        .and_then(|id| world.record(id))
        .filter(|record| record.tag == T(b"LGTM"))
        .and_then(|record| record.find(T(b"DATA")));
    let ambient = (if inherited || cell_light.is_none() {
        template.filter(|bytes| bytes.len() >= 24)
    } else {
        cell_light.filter(|bytes| bytes.len() >= 92)
    })
    .map_or(0.0, |bytes| {
        (f32::from(bytes[0]) + f32::from(bytes[1]) + f32::from(bytes[2])) / 3.0
    });
    let mut level = ambient * (100.0 / 255.0);
    let mut references = world.order().build_cell_index().references(cell).to_vec();
    references.extend(world.runtime_references_in_cell(cell));
    for id in references {
        let (base, placement, reference) = if let Some(reference) =
            world.record(id).filter(|record| record.tag == T(b"REFR"))
        {
            let Some(raw_base) = reference.find(T(b"NAME")).filter(|bytes| bytes.len() == 4) else {
                continue;
            };
            let raw = u32::from_le_bytes([raw_base[0], raw_base[1], raw_base[2], raw_base[3]]);
            (
                world.resolve_from(id, raw),
                world.ref_placement(id),
                Some(reference),
            )
        } else if let Some(runtime) = world.reference_ref(id) {
            let placement = soe_game::world::RefPlacement {
                position: runtime.position,
                rotation: runtime.rotation,
                scale: runtime.scale,
                visible: runtime.enabled && !runtime.deleted,
            };
            (runtime.base, Some(placement), None)
        } else {
            continue;
        };
        let Some(base) = base else { continue };
        let Some(base_record) = world.record(base).filter(|record| record.tag == T(b"LIGH")) else {
            continue;
        };
        let Some(data) = base_record
            .find(T(b"DATA"))
            .filter(|bytes| bytes.len() >= 48)
        else {
            continue;
        };
        let flags = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        if placement.is_some_and(|p| !p.visible)
            || reference.is_some_and(|record| record.flags & flags::INITIALLY_DISABLED != 0)
            || (placement.is_none() && flags & (1 << 5) != 0)
        {
            continue;
        }
        let radius = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as f32;
        if radius <= 0.0 {
            continue;
        }
        let position = placement.map_or_else(
            || {
                reference?
                    .find(T(b"DATA"))
                    .filter(|bytes| bytes.len() >= 12)
                    .map(|bytes| {
                        std::array::from_fn(|i| {
                            let start = i * 4;
                            f32::from_le_bytes(
                                bytes[start..start + 4].try_into().unwrap_or_default(),
                            )
                        })
                    })
            },
            |p| Some(p.position),
        );
        let Some(position) = position else { continue };
        let distance = length(sub(position, point));
        if distance < radius {
            let brightness = (f32::from(data[8]) + f32::from(data[9]) + f32::from(data[10])) / 3.0;
            let sign = if flags & (1 << 2) != 0 { -1.0 } else { 1.0 };
            let fade = base_record
                .find(T(b"FNAM"))
                .filter(|bytes| bytes.len() == 4)
                .map_or(1.0, |bytes| {
                    f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).clamp(0.0, 10.0)
                });
            level += sign * brightness * fade * (100.0 / 255.0) * (1.0 - distance / radius);
        }
    }
    level.clamp(0.0, 150.0)
}

#[cfg(test)]
mod cmb_32_tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

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
