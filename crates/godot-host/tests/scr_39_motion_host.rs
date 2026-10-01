// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use godot::classes::physics_server_3d::BodyMode;
use soe_godot_host::godot_physics::motion_type_to_body_mode;

#[test]
fn scr_39_motion_type_seven_maps_to_a_host_character_body_mode() {
    assert_eq!(motion_type_to_body_mode(7), Some(BodyMode::KINEMATIC));
    assert_eq!(motion_type_to_body_mode(4), Some(BodyMode::KINEMATIC));
    assert_eq!(motion_type_to_body_mode(8), None);
}
