// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::BlockRef;
use soe_physics::{
    controller_shape, generated_controller_shape, Collected, Iso, Layer, Phantom, Shape, Vec3,
};

#[test]
fn fmt_22_trigger_volume_never_claims_controller_or_solid_collider_status() {
    let trigger = Phantom {
        node: BlockRef(4),
        node_name: "TriggerVolume".into(),
        layer: Layer::TRIGGER,
        filter_flags: 0x42,
        group: 0x1234,
        transform: Iso::IDENTITY,
        shape: Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        },
    };
    let collected = Collected {
        colliders: Vec::new(),
        phantoms: vec![trigger],
        skipped: Vec::new(),
    };
    assert!(!collected.has_authored_controller());
    assert_eq!(collected.colliders.len(), 0);
    assert_eq!(collected.phantoms[0].filter_flags, 0x42);
    assert_eq!(collected.phantoms[0].group, 0x1234);
    assert_eq!(
        collected.phantoms[0].shape,
        Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 3.0)
        }
    );
    assert_eq!(
        controller_shape(&collected, 1.0).1,
        generated_controller_shape(1.0)
    );
}
