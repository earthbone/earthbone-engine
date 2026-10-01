// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::Transform;

#[test]
fn edt_15_places_reference_origin_at_collision_height() {
    let before = Transform {
        position: [12.0, -8.0, 240.0],
        rotation: [0.1, 0.2, 0.3],
    };
    let placed = before.drop_to_ground(-5.5).unwrap().placed();

    assert_eq!(placed.position, [12.0, -8.0, -5.5]);
    assert_eq!(placed.rotation, before.rotation);
}

#[test]
fn edt_15_restores_reference_transform_for_undo() {
    let before = Transform {
        position: [12.0, -8.0, 240.0],
        rotation: [0.1, 0.2, 0.3],
    };
    let drop = before.drop_to_ground(-5.5).unwrap();

    assert_ne!(drop.placed(), before);
    assert_eq!(drop.undo(), before);
}

#[test]
fn edt_15_rejects_non_finite_collision_height() {
    let before = Transform {
        position: [12.0, -8.0, 240.0],
        rotation: [0.1, 0.2, 0.3],
    };

    assert!(before.drop_to_ground(f32::NAN).is_none());
    assert!(before.drop_to_ground(f32::INFINITY).is_none());
    assert_eq!(before.position[2], 240.0);
}
