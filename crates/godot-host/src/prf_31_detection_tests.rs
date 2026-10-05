// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-31 checks spatial detection against the former full light scan.

use super::*;
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut ambient = vec![0_u8; 92];
    ambient[..3].copy_from_slice(&[60, 60, 60]);
    plugin.push_top(Record::new(T(b"CELL"), 1).sub(b"XCLL", ambient));
    plugin.push_top(Record::new(T(b"CELL"), 4));
    for (object, radius, color, flags, fade) in [
        (2, 100, 255, 0, 1.0_f32),
        (5, 200, 60, 4, 2.0),
        (6, 100, 255, 32, 1.0),
        (7, 100_000, 3, 0, 1.0),
    ] {
        let mut data = vec![0_u8; 48];
        data[4..8].copy_from_slice(&u32::to_le_bytes(radius));
        data[8..11].fill(color);
        data[12..16].copy_from_slice(&u32::to_le_bytes(flags));
        plugin.push_top(
            Record::new(T(b"LIGH"), object)
                .sub(b"DATA", data)
                .sub(b"FNAM", fade.to_le_bytes()),
        );
        let mut reference = Record::new(T(b"REFR"), object + 10)
            .sub(b"NAME", object.to_le_bytes())
            .sub(b"DATA", [0_u8; 24]);
        reference.placement.cell = Some(1);
        plugin.push_top(reference);
    }
    let mut disabled = Record::new(T(b"REFR"), 20)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    disabled.flags |= flags::INITIALLY_DISABLED;
    disabled.placement.cell = Some(1);
    plugin.push_top(disabled);
    let mut order = LoadOrder::new();
    order.add("Lights.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_spatial_light_matches_full_scan_and_current_reference_state() {
    let mut world = fixture();
    let points = [
        [0.0; 3],
        [99.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [-101.0, 0.0, 0.0],
        [4096.0, 0.0, 0.0],
    ];
    let compare = |world: &World| {
        let lights = LightSamples::new(world, &[id(1), id(4)]);
        for cell in [id(1), id(4)] {
            for point in points {
                assert_eq!(
                    lights.level(cell, point),
                    full_scan_light_level(world, cell, point),
                    "cell={cell} point={point:?}"
                );
            }
        }
    };
    compare(&world);
    world.reference(id(12)).unwrap().enabled = false;
    compare(&world);
    world.reference(id(12)).unwrap().enabled = true;
    world.reference(id(20)).unwrap().enabled = true;
    assert!(light_level(&world, id(1), [0.0; 3]) > 100.0);
    compare(&world);
    world.reference(id(12)).unwrap().position = [5000.0, 0.0, 0.0];
    world.reference(id(12)).unwrap().cell = Some(id(4));
    compare(&world);
    let spawned = world.place_at(id(2), id(15), false);
    compare(&world);
    world.reference(spawned).unwrap().enabled = false;
    compare(&world);
    world.reference(spawned).unwrap().deleted = true;
    compare(&world);
}

fn actor(object: u32, cell: u32, position: [f32; 3]) -> ContactActor {
    ContactActor {
        actor: id(object),
        cell: id(cell),
        position,
        rotation: [0.0; 3],
        scale: 1.0,
    }
}

#[test]
fn prf_31_pairs_use_range_and_one_light_sample_per_actor() {
    let actors = [
        actor(10, 1, [0.0; 3]),
        actor(11, 1, [10.0, 0.0, 0.0]),
        actor(12, 1, [0.0, 10.0, 0.0]),
        actor(13, 1, [1000.0, 0.0, 0.0]),
        actor(14, 4, [0.0; 3]),
    ];
    let mut samples = 0;
    let pairs = observations_in_range(
        &actors,
        &[],
        &BTreeMap::new(),
        100.0,
        |_, _| true,
        |_, _| {
            samples += 1;
            42.0
        },
    );
    assert_eq!(pairs.len(), 6);
    assert_eq!(samples, 3);
    assert!(pairs
        .iter()
        .all(|o| o.light_level == 42.0 && o.distance <= 100.0));
}

#[test]
fn prf_31_sound_source_keeps_distant_owner_without_duplicate_pairs() {
    let actors = [
        actor(10, 1, [0.0; 3]),
        actor(11, 1, [1000.0, 0.0, 0.0]),
        actor(12, 4, [0.0; 3]),
    ];
    let events = [
        DetectionEvent {
            owner: id(11),
            cell: id(1),
            position: [1.0, 0.0, 0.0],
            sound_level: 125.0,
        },
        DetectionEvent {
            owner: id(11),
            cell: id(1),
            position: [2.0, 0.0, 0.0],
            sound_level: 150.0,
        },
    ];
    let pairs = observations_in_range(
        &actors,
        &events,
        &BTreeMap::new(),
        100.0,
        |_, _| true,
        |_, _| 0.0,
    );
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].target, id(11));
    assert_eq!(pairs[0].action_sound, 150.0);
    assert_eq!(pairs[0].action_sound_distance, Some(2.0));
    assert!(!pairs[0].line_of_sight);
}

#[test]
fn prf_31_sparse_population_avoids_distant_pair_queries() {
    let actors = (10..5010)
        .map(|object| actor(object, 1, [object as f32 * 1000.0, 0.0, 0.0]))
        .collect::<Vec<_>>();
    let pairs = observations_in_range(
        &actors,
        &[],
        &BTreeMap::new(),
        100.0,
        |_, _| panic!("distant sight query"),
        |_, _| panic!("distant light query"),
    );
    assert!(pairs.is_empty());
}

#[test]
fn prf_31_vanilla_light_results_match_full_scan() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let mut world = World::from_shared_order(source.shared_order());
    let city_world = world.form("WhiterunWorld").unwrap();
    let mut city_cells = source
        .cell_index()
        .cells()
        .map(|(cell, _)| cell)
        .filter(|cell| source.exterior_world(*cell) == Some(city_world))
        .collect::<Vec<_>>();
    city_cells.sort_unstable();
    let tamriel = city_cells
        .into_iter()
        .find_map(|city| {
            world.cell_doors(city).into_iter().find_map(|(gate, far)| {
                let cell = world.reference(far?)?.cell?;
                let worldspace = source.exterior_world(cell)?;
                (world.record(worldspace)?.editor_id()?.eq("Tamriel")).then(|| {
                    println!("PRF31 gate={gate} Tamriel cell={cell}");
                    cell
                })
            })
        })
        .expect("Whiterun gate to Tamriel");
    let cells = [
        source.resolve_cell("WhiterunBanneredMare").unwrap(),
        source.resolve_cell("WhiterunOrigin").unwrap(),
        tamriel,
    ];
    for cell in cells {
        let samples = LightSamples::new(&world, &[cell]);
        for point in [
            [0.0; 3],
            [1000.0, -1000.0, 100.0],
            [-4096.0, 4096.0, 3000.0],
        ] {
            assert_eq!(
                samples.level(cell, point),
                full_scan_light_level(&world, cell, point)
            );
        }
    }
}

#[test]
fn prf_31_range_boundary_and_negative_buckets_keep_near_pairs() {
    let actors = [
        actor(10, 1, [-100.0, 0.0, 0.0]),
        actor(11, 1, [0.0; 3]),
        actor(12, 1, [0.001, 0.0, 0.0]),
    ];
    let pairs = observations_in_range(
        &actors,
        &[],
        &BTreeMap::new(),
        100.0,
        |_, _| true,
        |_, _| 0.0,
    );
    assert!(pairs
        .iter()
        .any(|o| o.observer == id(10) && o.target == id(11)));
    assert!(!pairs
        .iter()
        .any(|o| o.observer == id(10) && o.target == id(12)));
    assert_eq!(pairs.len(), 4);
}

#[test]
fn prf_31_invalid_range_performs_no_sight_or_light_queries() {
    let actors = [actor(10, 1, [0.0; 3]), actor(11, 1, [0.0; 3])];
    for range in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(observations_in_range(
            &actors,
            &[],
            &BTreeMap::new(),
            range,
            |_, _| panic!("invalid range sight"),
            |_, _| panic!("invalid range light")
        )
        .is_empty());
    }
}
