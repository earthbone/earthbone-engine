// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::CameraControls;
use soe_godot_host::camera::CameraHostState;
use soe_physics::Vec3;

#[test]
fn hst_17_cleared_or_mismatched_camera_target_uses_player_anchor() {
    let target = GlobalFormId {
        file: 3,
        object: 0x1234,
    };
    let other = GlobalFormId {
        file: 3,
        object: 0x1235,
    };
    let player = Vec3::new(1.0, 2.0, 3.0);
    let mut state = CameraHostState::default();
    state.apply(CameraControls {
        target: Some(target),
        ..CameraControls::default()
    });
    assert_eq!(state.anchor(player, 128.0, Some((other, Vec3::ZERO, 190.0))), (player, 128.0));

    state.apply(CameraControls::default());
    assert_eq!(state.target(), None);
    assert_eq!(state.anchor(player, 128.0, Some((target, Vec3::ZERO, 190.0))), (player, 128.0));
}
