// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-30 collision readiness, bounded spawn snapping and capsule locomotion.

use std::collections::BTreeSet;

use soe_esm::GlobalFormId;
use soe_godot_host::npc_grounding::NpcGrounding;
use soe_nif::types::BlockRef;
use soe_physics::{
    CharacterSettings, Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3,
};

const ACTOR: GlobalFormId = GlobalFormId { file: 0, object: 1 };
const CELL: GlobalFormId = GlobalFormId { file: 0, object: 2 };
const DT: f32 = 1.0 / 60.0;

fn floor(z: f32) -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: "floor".into(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: Iso::placement([0.0, 0.0, z - 1.0], [0.0; 3], 1.0),
        shape: Shape::Box {
            half_extents: Vec3::new(1024.0, 1024.0, 1.0),
        },
    }
}

#[test]
fn hst_30_spawn_waits_for_cell_collision_and_never_falls_below_floor() {
    for deferred in [false, true] {
        let mut physics = SimpleWorld::new();
        let mut grounding = NpcGrounding::default();
        if deferred {
            for _ in 0..120 {
                assert_eq!(
                    grounding.update(&mut physics, ACTOR, None, [0.0, 0.0, 80.0], 1.0, DT),
                    None
                );
                assert!(grounding.character(&physics, ACTOR).is_none());
            }
        }
        physics.add_static(&floor(0.0));
        let mut position = grounding
            .update(&mut physics, ACTOR, Some(CELL), [0.0, 0.0, 80.0], 1.0, 0.0)
            .unwrap();
        assert!(
            position[2].abs() < 0.01,
            "spawn snaps before presentation: {position:?}"
        );
        for _ in 0..600 {
            position = grounding
                .update(&mut physics, ACTOR, Some(CELL), position, 1.0, DT)
                .unwrap();
            assert!(
                position[2] >= -0.01,
                "NPC fell below the floor: {position:?}"
            );
        }
        assert!(grounding.character(&physics, ACTOR).unwrap().on_ground);
    }
}

#[test]
fn hst_30_spawn_snap_is_downward_and_bounded_to_128_units() {
    for (height, expected) in [(128.0, 0.0), (129.0, 129.0), (-10.0, -10.0), (-0.25, -0.25)] {
        let mut physics = SimpleWorld::new();
        physics.add_static(&floor(0.0));
        let position = NpcGrounding::default()
            .update(
                &mut physics,
                ACTOR,
                Some(CELL),
                [0.0, 0.0, height],
                1.0,
                0.0,
            )
            .unwrap();
        assert!(
            (position[2] - expected).abs() < 0.01,
            "{height}: {position:?}"
        );
    }
}

#[test]
fn hst_30_support_probe_prefers_the_floor_to_a_ceiling() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    physics.add_static(&floor(96.0));
    assert_eq!(
        soe_godot_host::npc_grounding::support_height(&physics, Vec3::new(0.0, 0.0, 1.0)),
        Some(0.0)
    );
}

#[test]
fn hst_30_gravity_catches_a_spawn_above_snap_range_without_stepping_player() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    let player =
        physics.create_character(CharacterSettings::default(), Vec3::new(300.0, 0.0, 500.0));
    let player_before = *physics.character(player).unwrap();
    let mut grounding = NpcGrounding::default();
    let mut position = [0.0, 0.0, 200.0];
    let mut fell = false;
    for _ in 0..180 {
        let next = grounding
            .update(&mut physics, ACTOR, Some(CELL), position, 1.0, DT)
            .unwrap();
        fell |= next[2] < position[2];
        assert!(next[2] >= -0.01);
        position = next;
    }
    assert!(fell && grounding.character(&physics, ACTOR).unwrap().on_ground);
    assert_eq!(*physics.character(player).unwrap(), player_before);
}

#[test]
fn hst_30_gravity_is_independent_of_unchanged_snapshot_height() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    let mut grounding = NpcGrounding::default();
    for _ in 0..180 {
        let position = grounding
            .update(&mut physics, ACTOR, Some(CELL), [0.0, 0.0, 200.0], 1.0, DT)
            .unwrap();
        assert!(position[2] >= -0.01);
    }
    let character = grounding.character(&physics, ACTOR).unwrap();
    assert!(character.on_ground && character.position.z.abs() < 0.01);
}

#[test]
fn hst_30_capsule_walks_stairs_and_sloped_terrain_with_gravity() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    for step in 0..4 {
        let mut collider = floor(24.0 * (step + 1) as f32);
        collider.transform = Iso::placement(
            [120.0 + step as f32 * 80.0, 0.0, 12.0 * (step + 1) as f32],
            [0.0; 3],
            1.0,
        );
        collider.shape = Shape::Box {
            half_extents: Vec3::new(40.0, 200.0, 12.0 * (step + 1) as f32),
        };
        physics.add_static(&collider);
    }
    let mut ramp = floor(0.0);
    ramp.shape = Shape::TriangleMesh {
        vertices: vec![
            Vec3::new(400.0, -200.0, 96.0),
            Vec3::new(800.0, -200.0, 176.0),
            Vec3::new(800.0, 200.0, 176.0),
            Vec3::new(400.0, 200.0, 96.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        materials: Vec::new(),
    };
    ramp.transform = Iso::IDENTITY;
    physics.add_static(&ramp);
    let mut grounding = NpcGrounding::default();
    let mut position = [0.0; 3];
    let mut climbed = 0.0_f32;
    for _ in 0..480 {
        let target = [position[0] + 110.0 * DT, position[1], position[2]];
        position = grounding
            .update(&mut physics, ACTOR, Some(CELL), target, 1.0, DT)
            .unwrap();
        assert!(position[2] >= -0.01);
        climbed = climbed.max(position[2]);
    }
    assert!(
        position[0] > 700.0 && climbed > 150.0,
        "stairs and terrain must be climbed: {position:?}, {climbed}"
    );
}

#[test]
fn hst_30_departure_teleport_and_scale_recreate_controller() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    let mut grounding = NpcGrounding::default();
    grounding
        .update(&mut physics, ACTOR, Some(CELL), [0.0; 3], 1.0, DT)
        .unwrap();
    let moved = grounding
        .update(
            &mut physics,
            ACTOR,
            Some(CELL),
            [300.0, 0.0, 80.0],
            0.5,
            0.0,
        )
        .unwrap();
    assert_eq!(moved, [300.0, 0.0, 0.0]);
    assert_eq!(
        grounding
            .character(&physics, ACTOR)
            .unwrap()
            .settings
            .radius,
        16.0
    );
    assert_eq!(
        grounding.update(&mut physics, ACTOR, None, moved, 0.5, DT),
        None
    );
    assert!(grounding.character(&physics, ACTOR).is_none());
    assert!(grounding
        .update(&mut physics, ACTOR, Some(CELL), moved, 0.5, DT)
        .is_some());
}

#[test]
fn hst_30_invalid_input_leaves_controller_unchanged() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    let mut grounding = NpcGrounding::default();
    grounding
        .update(&mut physics, ACTOR, Some(CELL), [0.0; 3], 1.0, DT)
        .unwrap();
    let before = *grounding.character(&physics, ACTOR).unwrap();
    for (position, scale, delta) in [
        ([f32::NAN, 0.0, 0.0], 1.0, DT),
        ([0.0; 3], 0.0, DT),
        ([0.0; 3], f32::MAX, DT),
        ([0.0; 3], 1.0, f32::NAN),
        ([0.0; 3], 1.0, -1.0),
    ] {
        assert_eq!(
            grounding.update(&mut physics, ACTOR, Some(CELL), position, scale, delta),
            None
        );
        assert_eq!(*grounding.character(&physics, ACTOR).unwrap(), before);
    }
}

#[test]
fn hst_30_coarse_horizontal_intent_is_blocked_by_cell_walls() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0));
    let mut wall = floor(0.0);
    wall.transform = Iso::placement([128.0, 0.0, 100.0], [0.0; 3], 1.0);
    wall.shape = Shape::Box {
        half_extents: Vec3::new(1.0, 200.0, 100.0),
    };
    physics.add_static(&wall);
    let mut grounding = NpcGrounding::default();
    grounding
        .update(&mut physics, ACTOR, Some(CELL), [0.0; 3], 1.0, DT)
        .unwrap();
    let position = grounding
        .update(
            &mut physics,
            ACTOR,
            Some(CELL),
            [120.0, 0.0, 0.0],
            1.0,
            0.25,
        )
        .unwrap();
    assert!(
        position[0] <= 96.01 && position[2] >= -0.01,
        "wall must stop the capsule: {position:?}"
    );
    grounding.retain(&mut physics, &BTreeSet::new());
    assert!(grounding.character(&physics, ACTOR).is_none());
}

#[test]
fn hst_30_installed_bannered_mare_actors_spawn_after_collision() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let cell = game.form("WhiterunBanneredMare").unwrap();
    game.change_cell(Some(cell));
    let player = game.player();
    let actors: Vec<_> = game
        .world_mut()
        .contact_actors()
        .into_iter()
        .filter(|actor| actor.actor != player)
        .collect();
    assert!(!actors.is_empty());
    let mut grounding = NpcGrounding::default();
    let mut empty = SimpleWorld::new();
    for actor in &actors {
        assert!(grounding
            .update(
                &mut empty,
                actor.actor,
                None,
                actor.position,
                actor.scale,
                DT
            )
            .is_none());
    }
    let mut physics = source.collide_cell_id(cell).unwrap().world;
    for actor in actors {
        let mut position = actor.position;
        for _ in 0..300 {
            position = grounding
                .update(
                    &mut physics,
                    actor.actor,
                    Some(cell),
                    position,
                    actor.scale,
                    DT,
                )
                .unwrap();
        }
        assert!(
            grounding
                .character(&physics, actor.actor)
                .unwrap()
                .on_ground,
            "{}: {position:?}",
            actor.actor
        );
        let feet = Vec3::new(position[0], position[1], position[2]);
        let support = physics
            .raycast(feet + Vec3::Z * 32.0, feet - Vec3::Z * 128.0)
            .unwrap();
        assert!(
            position[2] >= support.point.z - 32.0,
            "{} below support: {position:?}",
            actor.actor
        );
    }
}

#[test]
fn hst_30_installed_windowed_gate_path_npc_grounded() {
    let (Some(_data), Some(binary)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "res://tst_19_play_path.gd",
        ])
        .env("SOE_PLAY_PATH", "gate")
        .env("SOE_THREADED_SIMULATION", "0")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(root.join("target/hst_30_gate.log"), &log).unwrap();
    println!("{log}");
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS npc.grounded")
            && !log.contains("SCRIPT ERROR:"),
        "host exit {:?}",
        output.status.code()
    );
}
