// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verify the activated byte-loaded door path uses a moving physics body.

const ANIMATED_STATIC: &str = include_str!("../src/godot_animated_static.rs");
const GAMEPLAY: &str = include_str!("../src/godot_player/gameplay.rs");

#[test]
fn hst_01_vfs_loaded_door_collision_moves_with_the_swing() {
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
        "the activated door's byte loader must create a moving physics body"
    );
    assert!(byte_loader.contains("set_sync_to_physics(true)"));
    assert!(byte_loader.contains("create_trimesh_shape()"));
    assert!(!byte_loader.contains("create_trimesh_collision()"));
}
