// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::CameraControls;
use soe_godot_host::camera::{CameraHostState, CameraMode, CameraRig};
use soe_physics::Vec3;

#[test]
fn hst_17_host_camera_state_tracks_actor_target() {
    let target = GlobalFormId {
        file: 2,
        object: 0x1234,
    };
    let mut state = CameraHostState::default();
    state.apply(CameraControls {
        target: Some(target),
        ..CameraControls::default()
    });
    assert_eq!(state.target(), Some(target));
    let actor_feet = Vec3::new(500.0, 700.0, 120.0);
    let (anchor, height) = state.anchor(Vec3::ZERO, 128.0, Some((target, actor_feet, 180.0)));
    assert_eq!(anchor, actor_feet);
    assert_eq!(height, 180.0);
    let rig = CameraRig::default().with_mode(CameraMode::Third);
    let eye = rig.eye(anchor, height, None);
    assert_eq!((eye.position.x, eye.position.z), (530.0, 272.0));
}

#[test]
fn hst_17_host_camera_state_toggles_first_person_geometry() {
    let mut state = CameraHostState::default();
    assert!(state.first_person_geometry_visible());
    state.apply(CameraControls {
        first_person_geometry: false,
        ..CameraControls::default()
    });
    assert!(!state.first_person_geometry_visible());
    state.apply(CameraControls::default());
    assert!(state.first_person_geometry_visible());
}

#[test]
fn hst_17_host_sitting_rotation_offsets_third_person_camera() {
    let mut state = CameraHostState::default();
    state.apply(CameraControls {
        sitting_rotation_degrees: 90.0,
        ..CameraControls::default()
    });
    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    let eye = rig.eye(Vec3::ZERO, 128.0, None);
    assert!((eye.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!(
        eye.position.x < -100.0,
        "camera did not rotate around actor: {eye:?}"
    );
}

#[test]
fn hst_17_host_controls_reach_camera_pose_and_geometry_visibility() {
    let target = GlobalFormId {
        file: 4,
        object: 0x5678,
    };
    let target_feet = Vec3::new(120.0, -80.0, 24.0);
    let mut state = CameraHostState::default();
    state.apply(CameraControls {
        target: Some(target),
        first_person_geometry: false,
        sitting_rotation_degrees: 90.0,
    });

    let (anchor, height) = state.anchor(Vec3::ZERO, 128.0, Some((target, target_feet, 176.0)));
    assert_eq!((anchor, height), (target_feet, 176.0));
    assert!(!state.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(state.rendered_first_person_geometry_visible(CameraMode::Third));

    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    rig.set_camera_yaw_offset_degrees(state.sitting_rotation_degrees());
    let eye = rig.eye(anchor, height, None);
    assert!((eye.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!((eye.position.x - (anchor.x - rig.boom_length())).abs() < 1e-4);
    assert_eq!(state.target(), Some(target));
}

#[test]
fn scr_30_host_applies_game_camera_target_and_geometry_controls() {
    let actor = GlobalFormId {
        file: 3,
        object: 0x4321,
    };
    let mut state = CameraHostState::default();
    state.apply(CameraControls {
        target: Some(actor),
        first_person_geometry: false,
        ..CameraControls::default()
    });
    assert_eq!(state.target(), Some(actor));
    assert!(!state.first_person_geometry_visible());
}

#[test]
fn scr_30_rendered_geometry_visibility_respects_pov_and_native_flag() {
    let mut state = CameraHostState::default();
    assert!(state.rendered_first_person_geometry_visible(CameraMode::First));
    state.apply(CameraControls {
        first_person_geometry: false,
        ..CameraControls::default()
    });
    assert!(!state.rendered_first_person_geometry_visible(CameraMode::First));
    assert!(state.rendered_first_person_geometry_visible(CameraMode::Third));
    state.apply(CameraControls::default());
    assert!(state.rendered_first_person_geometry_visible(CameraMode::First));
}
