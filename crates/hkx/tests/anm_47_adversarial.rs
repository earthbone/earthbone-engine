// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

#[test]
fn transform_vector_rotates_and_translates_xyz_while_preserving_w() {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let v = b.array_structs(root, 0, 24, 1);
    b.string(v, 8, "hkbBehaviorGraph");
    b.ptr(v, 16, graph);
    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["Out"]);
    let info = b.array_structs(data, 32, 6, 1);
    b.set_i8(info, 4, 7);
    b.array_i32(values, 16, &[0]);
    let quad: [f32; 4] = [0.0; 4];
    let _ = quad;
    b.array_bytes(
        values,
        32,
        &[
            0.0f32.to_le_bytes(),
            0.0f32.to_le_bytes(),
            0.0f32.to_le_bytes(),
            0.0f32.to_le_bytes(),
        ]
        .concat(),
        1,
    );
    let w = b.object("hkbModifierGenerator", 88);
    let m = b.object("hkbTransformVectorModifier", 160);
    b.ptr(graph, 128, w);
    b.ptr(w, 72, m);
    b.set_bool(m, 72, true);
    b.set_vec4(m, 80, [0.0, 0.0, 0.70710677, 0.70710677]);
    b.set_vec4(m, 96, [2.0, 3.0, 4.0, 99.0]);
    b.set_vec4(m, 112, [1.0, 0.0, 0.0, 7.0]);
    b.set_bool(m, 144, false);
    b.set_bool(m, 145, false);
    b.set_bool(m, 146, true);
    let set = b.object("hkbVariableBindingSet", 40);
    b.ptr(m, 16, set);
    let bind = b.array_structs(set, 16, 40, 1);
    b.string(bind, 0, "vectorOut");
    b.set_i32(bind, 28, 0);
    b.set_i32(set, 32, -1);
    let g = BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&g, &clips, vec![]);
    let _ = vm.update(0.0);
    let output = vm.variables.quad(0).unwrap();
    assert!((output[0] - 2.0).abs() < 0.001);
    assert!((output[1] - 4.0).abs() < 0.001);
    assert!((output[2] - 4.0).abs() < 0.001);
    assert_eq!(output[3], 7.0);
}
