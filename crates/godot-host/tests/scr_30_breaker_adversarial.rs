// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::camera::{CameraHostState, CameraMode};
use soe_physics::Vec3;

#[test]
fn scr_30_visibility_setting_survives_camera_mode_changes() {
    let mut state = CameraHostState::default();
    let hidden = soe_game::world::CameraControls {
        first_person_geometry: false,
        ..soe_game::world::CameraControls::default()
    };
    state.apply(hidden);
    assert!(!state.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(state.rendered_first_person_geometry_visible(CameraMode::Third));

    state.apply(soe_game::world::CameraControls::default());
    assert!(state.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(state.rendered_first_person_geometry_visible(CameraMode::Third));
}

#[test]
fn scr_30_missing_camera_target_pose_falls_back_to_player() {
    let target = GlobalFormId {
        file: 1,
        object: 0x123,
    };
    let player_feet = Vec3::new(10.0, 20.0, 30.0);
    let mut state = CameraHostState::default();
    state.apply(soe_game::world::CameraControls {
        target: Some(target),
        ..soe_game::world::CameraControls::default()
    });

    assert_eq!(state.anchor(player_feet, 128.0, None), (player_feet, 128.0));
}
