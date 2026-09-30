// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn get_up_ignores_invalid_selected_indices_without_touching_other_bones() {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let v = b.array_structs(root, 0, 24, 1);
    b.string(v, 8, "hkbBehaviorGraph");
    b.ptr(v, 16, graph);
    let wrap = b.object("hkbModifierGenerator", 88);
    let m = b.object("hkbGetUpModifier", 128);
    b.ptr(graph, 128, wrap);
    b.ptr(wrap, 72, m);
    b.set_bool(m, 72, true);
    b.set_f32(m, 80, 0.0);
    b.set_f32(m, 84, 1.0);
    b.set_f32(m, 88, 0.0);
    b.set_f32(m, 96, 1.0);
    b.set_f32(m, 100, 1.0);
    b.set_i16(m, 104, -1);
    b.set_i16(m, 106, 99);
    b.set_i16(m, 108, 2);
    let g = BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap();
    let pose = vec![QsTransform::IDENTITY; 3];
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&g, &clips, pose.clone());
    let out = vm.update(0.1).pose.clone();
    assert_eq!(out[0], pose[0]);
    assert_eq!(out[1], pose[1]);
    assert_ne!(out[2].rotation, pose[2].rotation);
}
