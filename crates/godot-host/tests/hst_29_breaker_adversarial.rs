// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial locomotion samples for HST-29.

use soe_game::world::RefPlacement;
use soe_godot_host::actor::NpcMotion;
use std::path::PathBuf;

fn placement(position: [f32; 3], heading: f32) -> RefPlacement {
    RefPlacement {
        position,
        rotation: [0.0, 0.0, heading],
        scale: 1.0,
        visible: true,
    }
}

#[test]
fn vertical_only_displacement_does_not_select_walk_locomotion() {
    let before = placement([0.0; 3], 0.0);
    let after = placement([0.0, 0.0, 24.0], 0.5);
    let motion = NpcMotion::between(before, after, 0.5).unwrap();

    assert_eq!(motion.speed, 0.0, "vertical movement is not horizontal locomotion");
    assert_eq!(motion.direction, 0.0);
}

#[test]
fn invalid_world_coordinates_do_not_produce_animation_inputs() {
    let before = placement([0.0; 3], 0.0);
    for after in [
        placement([f32::NAN, 0.0, 0.0], 0.0),
        placement([f32::INFINITY, 0.0, 0.0], 0.0),
        placement([0.0; 3], f32::NAN),
    ] {
        assert!(NpcMotion::between(before, after, 1.0).is_none());
    }
}

#[test]
fn installed_play_path_reports_npc_motion_and_pose() {
    let (Some(data), Some(binary)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script", "hst_29_walkthrough_test.gd"])
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
    for line in log.lines().filter(|line| line.starts_with("PLAY_PATH") || line.starts_with("RESULT:")) {
        println!("{line}");
    }
    assert!(
        output.status.success()
            && log.contains("PLAY_PATH npc.pose pass")
            && log.contains("PLAY_PATH npc.moves pass")
            && log.contains("RESULT: PASS hst_29"),
        "installed NPC play path failed: {}\n{log}",
        output.status
    );
}
