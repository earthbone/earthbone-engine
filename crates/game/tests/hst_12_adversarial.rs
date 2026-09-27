// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn fixture() -> (World, GlobalFormId) {
    let id = GlobalFormId { file: 0, object: 0x123 };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x124));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), id.object)
            .sub(Tag::new(b"NAME"), 0x124_u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), Transform { position: [1.0, 2.0, 3.0], rotation: [0.0; 3] }.to_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("fixture.esp", plugin).unwrap();
    (World::new(order), id)
}

#[test]
fn hst_12_deleted_reference_rejects_a_late_physics_pose() {
    let (mut world, id) = fixture();
    world.reference(id).unwrap().deleted = true;
    assert!(!world.set_host_ref_pose(id, [10.0; 3], [0.5; 3]));
    assert_eq!(world.ref_placement(id).unwrap().position, [1.0, 2.0, 3.0]);
}

#[test]
fn hst_12_missing_reference_does_not_materialize_from_a_late_pose() {
    let (mut world, id) = fixture();
    let missing = GlobalFormId { file: id.file, object: 0x999 };
    assert!(!world.set_host_ref_pose(missing, [10.0; 3], [0.0; 3]));
    assert!(world.reference_ref(missing).is_none());
}

#[test]
fn hst_12_nonfinite_rotation_is_rejected_without_mutating_pose() {
    let (mut world, id) = fixture();
    world.reference(id).unwrap();
    assert!(!world.set_host_ref_pose(id, [10.0; 3], [0.0, f32::INFINITY, 0.0]));
    assert_eq!(world.ref_placement(id).unwrap().position, [1.0, 2.0, 3.0]);
}
