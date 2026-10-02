#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used, dead_code)]

use std::time::Duration;

#[derive(Debug)]
pub enum DataSourceError {
    WorkerPanicked,
    WorkerPoolBuild(rayon::ThreadPoolBuildError),
}

mod data_source {
    pub use crate::DataSourceError;
}

pub use soe_godot_host::mesh;

#[path = "../src/data_source/workers.rs"]
mod workers;

use soe_esm::LoadOrder;
use soe_game::determinism::{compare, Divergence, TickHash};
use soe_game::Game;
use soe_godot_host::mesh::{
    build_surface, Aabb, MaterialDesc, MeshData, MeshOptions, SurfaceArrays,
};
use soe_godot_host::units::GodotPos;
use workers::WorkerPools;

fn feed(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn feed_values(hash: &mut u64, tag: u32, values: impl IntoIterator<Item = u32>) {
    feed(hash, tag);
    let values: Vec<_> = values.into_iter().collect();
    feed(hash, values.len() as u32);
    for value in values {
        feed(hash, value);
    }
}

fn hash_surfaces(surfaces: &[SurfaceArrays]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    feed(&mut hash, surfaces.len() as u32);
    for surface in surfaces {
        feed_values(
            &mut hash,
            1,
            surface
                .positions
                .iter()
                .flatten()
                .map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            2,
            surface
                .normals
                .iter()
                .flatten()
                .map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            3,
            surface.tangents.iter().map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            4,
            surface.uvs.iter().flatten().map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            5,
            surface.indices.iter().map(|value| *value as u32),
        );
        feed_values(
            &mut hash,
            6,
            surface.bones.iter().map(|value| *value as u32),
        );
        feed_values(
            &mut hash,
            7,
            surface.weights.iter().map(|value| value.to_bits()),
        );
        feed(&mut hash, 8);
        for value in [
            surface.bounds.min.x,
            surface.bounds.min.y,
            surface.bounds.min.z,
            surface.bounds.max.x,
            surface.bounds.max.y,
            surface.bounds.max.z,
        ] {
            feed(&mut hash, value.to_bits());
        }
    }
    hash
}

fn build(value: u64) -> SurfaceArrays {
    let x = (value % 10_000) as f32;
    let mesh = MeshData {
        positions: vec![[x, 0.0, 0.0], [x + 3.0, 0.0, 0.0], [x, 0.0, 3.0]],
        indices: vec![0, 1, 2],
        material: MaterialDesc::default(),
        ..MeshData::default()
    };
    build_surface(&mesh, MeshOptions::default()).expect("valid triangle builds")
}

#[test]
fn tst_02_out_of_order_workers_match_serial_results_across_batch_shapes() {
    let pools = WorkerPools::default();
    for (len, workers) in [(0, 8), (1, 8), (2, 4), (31, 4), (64, 7)] {
        let input: Vec<u64> = (0..len).map(|value| value * 97 + 11).collect();
        let expected: Vec<_> = input.iter().map(|value| build(*value)).collect();
        let actual = pools
            .ordered_map(&input, workers, |value| {
                std::thread::sleep(Duration::from_micros((64 - value % 64) + 1));
                build(*value)
            })
            .expect("workers return a complete batch");
        assert_eq!(actual, expected, "len={len}, workers={workers}");
    }
}

#[test]
fn tst_02_hash_distinguishes_surface_boundaries() {
    fn bounds(min: f32, max: f32) -> Aabb {
        Aabb {
            min: GodotPos {
                x: min,
                y: min,
                z: min,
            },
            max: GodotPos {
                x: max,
                y: max,
                z: max,
            },
        }
    }

    let mut single = build(11);
    single.positions = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];
    single.normals.clear();
    single.tangents.clear();
    single.uvs.clear();
    single.indices.clear();
    single.bones.clear();
    single.weights.clear();
    single.bounds = bounds(0.0, 2.0);

    let mut first = single.clone();
    first.positions = vec![[0.0, 0.0, 0.0]];
    first.bounds = Aabb {
        min: GodotPos {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        max: GodotPos {
            x: 2.0,
            y: 2.0,
            z: 2.0,
        },
    };
    let mut second = single.clone();
    second.positions.clear();
    let split = [first, second];

    assert_ne!(
        hash_surfaces(std::slice::from_ref(&single)),
        hash_surfaces(&split),
        "surface boundaries are part of the host builder result"
    );
}

#[test]
fn tst_02_host_hashes_are_seeded_per_tick_and_name_only_the_changed_tick() {
    fn run(seed: u64) -> Vec<TickHash> {
        let pools = WorkerPools::default();
        let mut game = Game::new(LoadOrder::new());
        game.record_hashes();
        let mut random = seed;
        let mut log = Vec::new();
        for tick in 0..8 {
            game.tick(1.0 / 60.0);
            let inputs: Vec<_> = (0..31)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random
                })
                .collect();
            let serial: Vec<_> = inputs.iter().map(|value| build(*value)).collect();
            let parallel = pools
                .ordered_map(&inputs, 4, |value| {
                    std::thread::sleep(Duration::from_micros((value % 97) + 1));
                    build(*value)
                })
                .expect("workers return a complete batch");
            assert_eq!(parallel, serial, "tick {tick} retains input order");
            let game_tick = game.hash_log().last().expect("game tick was recorded");
            assert_eq!(game_tick.tick, tick);
            log.push(
                game_tick
                    .clone()
                    .with_host_builder_hash(hash_surfaces(&parallel)),
            );
        }
        log
    }

    let first = run(0x5eed);
    let repeated = run(0x5eed);
    assert_eq!(compare(&first, &repeated), Ok(()));
    assert!(first.iter().all(|tick| {
        tick.parts
            .last()
            .is_some_and(|(name, _)| *name == "host.builders")
    }));

    let mut changed = repeated;
    let host_hash = changed[5].parts.last().expect("host hash is attached").1;
    changed[5] = changed[5].clone().with_host_builder_hash(host_hash ^ 1);
    assert_eq!(
        compare(&first, &changed),
        Err(Divergence::AtTick {
            tick: 5,
            subsystems: vec!["host.builders"],
        })
    );
}
