// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_godot_host::camera::{CameraHostState, CameraMode, CameraRig};
use soe_game::world::CameraControls;
use soe_physics::Vec3;

#[test]
fn hst_17_non_player_target_moves_third_person_camera_anchor() {
    let target = GlobalFormId {
        file: 2,
        object: 0x12345,
    };
    let feet = Vec3::new(120.0, -80.0, 24.0);
    let controls = CameraControls {
        target: Some(target),
        ..CameraControls::default()
    };
    let mut host = CameraHostState::default();
    host.apply(controls);

    let anchor = host.anchor(
        Vec3::ZERO,
        128.0,
        Some((target, feet, 176.0)),
    );
    let mut rig = CameraRig::default().with_mode(CameraMode::Third);
    let targeted_eye = rig.eye(anchor.0, anchor.1, None);
    let player_eye = rig.eye(Vec3::ZERO, 128.0, None);

    assert_eq!(host.target(), Some(target));
    assert_eq!(anchor, (feet, 176.0));
    assert_ne!(targeted_eye.position, player_eye.position);
}
