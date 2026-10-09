// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Malformed controller graph checks grounded in nifly Animation.cpp and OpenMW controller.cpp.

#[path = "common/mod.rs"]
mod common;

use common::NifWriter;
use soe_nif::object_animation::ObjectAnimation;

#[test]
fn rnd_23_manager_rejects_a_dangling_sequence_link() {
    let mut w = NifWriter::new(100);
    let node = w.ni_node("Root", [0.0; 3], common::IDENTITY, 1.0, &[]);
    let root = w.add("NiNode", node);
    let manager = w.reserve("NiControllerManager");
    let mut bytes = common::Buf::default();
    bytes
        .i32(-1)
        .u16(8)
        .f32(1.0)
        .f32(0.0)
        .f32(0.0)
        .f32(1.0)
        .i32(root)
        .bool(false)
        .refs(&[77])
        .i32(-1);
    w.fill(manager, bytes.0);
    w.roots = vec![root];

    let file = soe_nif::parse(&w.finish()).unwrap();
    assert!(ObjectAnimation::from_file(&file).is_err());
}
