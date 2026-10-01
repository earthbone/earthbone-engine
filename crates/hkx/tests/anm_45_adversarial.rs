// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn powered_ragdoll_controls_emit_each_authored_bone_and_weight() {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let v = b.array_structs(root, 0, 24, 1);
    b.string(v, 8, "hkbBehaviorGraph");
    b.ptr(v, 16, graph);
    let w = b.object("hkbModifierGenerator", 88);
    let m = b.object("hkbPoweredRagdollControlsModifier", 144);
    let bones = b.object("hkbBoneIndexArray", 64);
    let weights = b.object("hkbBoneWeightArray", 64);
    b.ptr(graph, 128, w);
    b.ptr(w, 72, m);
    b.set_bool(m, 72, true);
    for (index, value) in [120.0, 0.7, 0.2, 3.0, 0.5].into_iter().enumerate() {
        b.set_f32(m, 80 + 4 * u32::try_from(index).unwrap(), value);
    }
    b.ptr(m, 112, bones);
    b.set_i16(m, 120, 1);
    b.set_i16(m, 122, 2);
    b.set_i16(m, 124, 3);
    b.set_i8(m, 126, 4);
    b.array_i16(bones, 48, &[2, 4]);
    b.ptr(m, 128, weights);
    b.array_f32(weights, 48, &[0.25, 0.75]);
    let g = BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&g, &clips, vec![QsTransform::IDENTITY; 5]);
    let output = vm.update(1.0 / 60.0).ragdoll_controls.clone();
    assert_eq!(output.len(), 2);
    assert_eq!((output[0].bone, output[0].weight), (2, 0.25));
    assert_eq!((output[1].bone, output[1].weight), (4, 0.75));
    assert_eq!(output[0].control.max_force, 120.0);
    assert_eq!(output[0].control.tau, 0.7);
    assert_eq!(output[0].control.damping, 0.2);
    assert_eq!(output[0].control.proportional_recovery_velocity, 3.0);
    assert_eq!(output[0].control.constant_recovery_velocity, 0.5);
    assert_eq!(output[0].world_from_model.pose_matching_bones, [1, 2, 3]);
    assert_eq!(output[0].world_from_model.mode, 4);
}
