// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-36 boundary checks for the declared real-valued speed output.

use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn graph() -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    b.string(graph, 56, "speed sampler edge test");
    b.set_i8(graph, 72, 1);

    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["goalSpeed", "speedOut"]);
    let infos = b.array_structs(data, 32, 6, 2);
    b.set_i8(infos, 4, 4);
    b.set_i8(infos + 6, 4, 4);
    b.array_i32(
        values,
        16,
        &[0.0f32.to_bits() as i32, 0.0f32.to_bits() as i32],
    );

    let root = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, root);
    b.string(root, 56, "root");
    let sampler = b.object("BSSpeedSamplerModifier", 96);
    b.string(sampler, 56, "speed sampler");
    b.set_bool(sampler, 72, true);
    b.set_i32(sampler, 80, 0);
    b.set_f32(sampler, 84, 0.0);
    b.set_f32(sampler, 88, 0.0);
    b.set_f32(sampler, 92, 0.0);
    b.ptr(root, 72, sampler);

    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(sampler, 16, bindings);
    let bind = b.array_structs(bindings, 16, 40, 2);
    for (offset, (path, index)) in [("goalSpeed", 0), ("speedOut", 1)].into_iter().enumerate() {
        let item = bind + offset as u32 * 40;
        b.string(item, 0, path);
        b.set_i32(item, 28, index);
    }
    b.set_i32(bindings, 32, -1);
    b.finish()
}

#[test]
fn anm_36_sampler_preserves_signed_zero_and_subnormal_goal_speed_bits() {
    let bytes = graph();
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    let goal = vm.variables.index("goalSpeed").unwrap();
    let output = vm.variables.index("speedOut").unwrap();

    for bits in [(-0.0f32).to_bits(), f32::from_bits(1).to_bits()] {
        let speed = f32::from_bits(bits);
        assert!(vm.variables.set_real(goal, speed));
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.real(output).unwrap().to_bits(), bits);
    }
}
