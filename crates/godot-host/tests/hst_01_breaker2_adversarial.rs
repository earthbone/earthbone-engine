// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The standalone HST-01 byte loader retains physics-synchronized collision.

const ANIMATED_STATIC: &str = include_str!("../src/godot_animated_static.rs");

#[test]
fn hst_01_standalone_byte_loaded_doors_use_physics_synchronized_moving_collision() {
    let byte_loader = ANIMATED_STATIC
        .split("pub fn load_nif_bytes(")
        .nth(1)
        .expect("byte loader exists")
        .split("pub fn open(")
        .next()
        .expect("byte loader ends before open");
    assert!(
        byte_loader.contains("AnimatableBody3D"),
        "byte-loaded doors must use AnimatableBody3D so their swing affects contacted bodies"
    );
    assert!(byte_loader.contains("set_sync_to_physics(true)"));
    assert!(byte_loader.contains("create_trimesh_shape()"));
}
