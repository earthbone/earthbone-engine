// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::Motion;
use soe_game::world::ContactActor;
use soe_game::World;
use soe_godot_host::detection::{light_level, observations_in_range, LightSamples};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn light_world(off_by_default: bool) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1));
    let mut data = vec![0_u8; 48];
    data[4..8].copy_from_slice(&256_u32.to_le_bytes());
    data[8..11].fill(255);
    let flags = if off_by_default { 1_u32 << 5 } else { 0 };
    data[12..16].copy_from_slice(&flags.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    let mut placed = vec![0_u8; 24];
    placed[..4].copy_from_slice(&10.0_f32.to_le_bytes());
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", placed);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("OffByDefault.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_off_by_default_light_stays_off_when_reference_state_is_loaded() {
    let mut world = light_world(true);
    assert_eq!(light_level(&world, id(1), [10.0, 0.0, 0.0]), 0.0);
    let reference = world.reference(id(3)).unwrap();
    assert!(reference.enabled);
    assert_eq!(light_level(&world, id(1), [10.0, 0.0, 0.0]), 0.0);
}

#[test]
fn prf_31_runtime_spawned_light_joins_the_next_cell_sample() {
    let mut world = light_world(false);
    let before = light_level(&world, id(1), [10.0, 0.0, 0.0]);
    let spawned = world.place_at(id(2), id(3), false);
    assert!(world.reference_ref(spawned).is_some());
    let after = light_level(&world, id(1), [10.0, 0.0, 0.0]);
    assert!(
        after > before,
        "a runtime LIGH placement contributes: {after} <= {before}"
    );
}

#[test]
fn prf_31_spatial_light_grid_matches_full_scan_at_negative_and_radius_edges() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1));
    let mut data = vec![0_u8; 48];
    data[4..8].copy_from_slice(&1025_u32.to_le_bytes());
    data[8..11].copy_from_slice(&[255, 100, 32]);
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    for (object, x) in [(3_u32, -2048.0_f32), (4, -1024.0), (5, 0.0)] {
        let mut placement = vec![0_u8; 24];
        placement[..4].copy_from_slice(&x.to_le_bytes());
        let mut reference = Record::new(Tag(*b"REFR"), object)
            .sub(b"NAME", 2_u32.to_le_bytes())
            .sub(b"DATA", placement);
        reference.placement.cell = Some(1);
        plugin.push_top(reference);
    }
    let mut order = LoadOrder::new();
    order.add("LightGrid.esm", plugin).unwrap();
    let world = World::new(order);
    let samples = LightSamples::new(&world, &[id(1)]);
    for x in [-3073.0, -3072.0, -2048.0, -1024.0, -1023.0, 0.0, 1024.0] {
        let point = [x, 0.0, 0.0];
        let intensity = (255.0 + 100.0 + 32.0) / 3.0 * (100.0 / 255.0);
        let expected = [-2048.0_f32, -1024.0, 0.0]
            .into_iter()
            .map(|source| {
                let distance = (source - x).abs();
                if distance < 1025.0 {
                    intensity * (1.0 - distance / 1025.0)
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            .clamp(0.0, 150.0);
        assert!((samples.level(id(1), point) - expected).abs() < 0.001);
        assert!((light_level(&world, id(1), point) - expected).abs() < 0.001);
    }
}

#[test]
fn prf_31_observation_batch_samples_light_once_per_target_form() {
    let actors = [
        ContactActor {
            actor: id(10),
            cell: id(1),
            position: [0.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: 1.0,
        },
        ContactActor {
            actor: id(11),
            cell: id(1),
            position: [1.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: 1.0,
        },
        ContactActor {
            actor: id(12),
            cell: id(1),
            position: [2.0, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: 1.0,
        },
    ];
    let mut samples = 0;
    let observations = observations_in_range(
        &actors,
        &[],
        &std::collections::BTreeMap::<GlobalFormId, Motion>::new(),
        10.0,
        |_, _| true,
        |_, _| {
            samples += 1;
            25.0
        },
    );
    assert_eq!(observations.len(), 6);
    assert_eq!(
        samples, 3,
        "each target receives one light sample per batch"
    );
}
