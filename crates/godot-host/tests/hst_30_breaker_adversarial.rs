// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial NPC spawn snapping for HST-30.

use soe_esm::GlobalFormId;
use soe_godot_host::npc_grounding::NpcGrounding;
use soe_nif::types::BlockRef;
use soe_physics::{Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3};

const ACTOR: GlobalFormId = GlobalFormId { file: 0, object: 1 };
const CELL: GlobalFormId = GlobalFormId { file: 0, object: 2 };

fn floor() -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: "floor".into(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: Iso::placement([0.0, 0.0, -1.0], [0.0; 3], 1.0),
        shape: Shape::Box {
            half_extents: Vec3::new(128.0, 128.0, 1.0),
        },
    }
}

#[test]
fn spawn_snap_includes_the_128_unit_limit_but_does_not_pull_from_above_it() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor());
    let at_limit = NpcGrounding::default()
        .update(&mut physics, ACTOR, Some(CELL), [0.0, 0.0, 50.0], 1.0, 0.0)
        .unwrap();
    assert!(at_limit[2].abs() < 0.01, "spawn should reach the floor: {at_limit:?}");

    let above_limit = NpcGrounding::default()
        .update(&mut physics, ACTOR, Some(CELL), [0.0, 0.0, 129.0], 1.0, 0.0)
        .unwrap();
    assert!((above_limit[2] - 129.0).abs() < 0.01);
}

#[test]
fn absent_cell_collision_defers_body_until_collision_is_available() {
    let mut physics = SimpleWorld::new();
    let mut grounding = NpcGrounding::default();
    assert_eq!(grounding.update(&mut physics, ACTOR, None, [0.0, 0.0, 80.0], 1.0, 0.0), None);
    assert!(grounding.character(&physics, ACTOR).is_none());

    physics.add_static(&floor());
    let position = grounding
        .update(&mut physics, ACTOR, Some(CELL), [0.0, 0.0, 80.0], 1.0, 0.0)
        .unwrap();
    assert!(position[2].abs() < 0.01);
    assert!(grounding.character(&physics, ACTOR).is_some());
}

#[test]
fn installed_play_path_reports_grounded_npcs_at_each_stop() {
    let (Some(data), Some(binary)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script", "tst_19_play_path.gd"])
        .env("SKYRIM_DATA", data)
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
    for line in log.lines().filter(|line| line.contains("npc.grounded") || line.starts_with("RESULT:")) {
        println!("{line}");
    }
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS npc.grounded")
            && !log.contains("SCRIPT ERROR:"),
        "installed grounded NPC play path failed: {}\n{log}",
        output.status
    );
}
