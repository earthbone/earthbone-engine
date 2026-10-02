// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check that the host activation path builds moving collision for byte-loaded doors.

const ANIMATED_STATIC: &str = include_str!("../src/godot_animated_static.rs");
const GAMEPLAY: &str = include_str!("../src/godot_player/gameplay.rs");

#[test]
fn hst_01_byte_loaded_swing_doors_use_physics_synchronized_moving_collision() {
    let activation = GAMEPLAY
        .split("fn open_swing_door(")
        .nth(1)
        .expect("swing door activation exists");
    assert!(activation.contains("load_nif_bytes(packed)"));

    let byte_loader = ANIMATED_STATIC
        .split("pub fn load_nif_bytes(")
        .nth(1)
        .expect("byte loader exists")
        .split("pub fn open(")
        .next()
        .expect("byte loader ends before open");
    assert!(
        byte_loader.contains("AnimatableBody3D"),
        "activated doors must use AnimatableBody3D so their swing affects contacted bodies"
    );
    assert!(byte_loader.contains("set_sync_to_physics(true)"));
    assert!(byte_loader.contains("create_trimesh_shape()"));
}
