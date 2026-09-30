// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-12 host pose writes use the same reference fields as save and render.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn fixture() -> (World, GlobalFormId) {
    let id = GlobalFormId {
        file: 0,
        object: 0x123,
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x124));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), id.object)
            .sub(Tag::new(b"NAME"), 0x124_u32.to_le_bytes())
            .sub(
                Tag::new(b"DATA"),
                Transform {
                    position: [1.0, 2.0, 3.0],
                    rotation: [0.0; 3],
                }
                .to_bytes(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("fixture.esp", plugin).unwrap();
    (World::new(order), id)
}

#[test]
fn hst_12_simulated_pose_updates_shared_world_and_save() {
    let (mut world, id) = fixture();
    assert!(world.set_host_ref_pose(id, [10.0, 20.0, 30.0], [0.1, 0.2, 0.3]));
    let placement = world.ref_placement(id).unwrap();
    assert_eq!(placement.position, [10.0, 20.0, 30.0]);
    assert_eq!(placement.rotation, [0.1, 0.2, 0.3]);
    let saved = world.save_state();
    let (mut restored, _) = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.ref_placement(id), Some(placement));
}

#[test]
fn hst_12_disabled_reference_rejects_late_body_pose() {
    let (mut world, id) = fixture();
    world.reference(id).unwrap().enabled = false;
    assert!(!world.set_host_ref_pose(id, [10.0; 3], [0.0; 3]));
    assert_eq!(world.ref_placement(id).unwrap().position, [1.0, 2.0, 3.0]);
}

#[test]
fn hst_12_invalid_pose_does_not_change_world() {
    let (mut world, id) = fixture();
    assert!(!world.set_host_ref_pose(id, [f32::NAN, 0.0, 0.0], [0.0; 3]));
    assert!(world.ref_placement(id).is_none());
}
