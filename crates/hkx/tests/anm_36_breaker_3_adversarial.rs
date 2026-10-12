// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-36 field type mismatch check.

use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn graph_with_integer_speed_output() -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    b.string(graph, 56, "speed sampler type mismatch");
    b.set_i8(graph, 72, 1);

    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["speedOut"]);
    let infos = b.array_structs(data, 32, 6, 1);
    b.set_i8(infos, 4, 3);
    b.array_i32(values, 16, &[17]);

    let root = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, root);
    b.string(root, 56, "root");
    let sampler = b.object("BSSpeedSamplerModifier", 96);
    b.string(sampler, 56, "speed sampler");
    b.set_bool(sampler, 72, true);
    b.set_i32(sampler, 80, -1);
    b.set_f32(sampler, 84, 0.0);
    b.set_f32(sampler, 88, 123.75);
    b.set_f32(sampler, 92, 0.0);
    b.ptr(root, 72, sampler);

    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(sampler, 16, bindings);
    let bind = b.array_structs(bindings, 16, 40, 1);
    b.string(bind, 0, "speedOut");
    b.set_i32(bind, 28, 0);
    b.set_i32(bindings, 32, -1);
    b.finish()
}

#[test]
fn anm_36_rejects_real_speed_out_binding_to_int32_variable() {
    let bytes = graph_with_integer_speed_output();
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let mut vm = Interpreter::new(&graph, &ClipLibrary::new(), vec![QsTransform::IDENTITY]);

    assert_eq!(
        graph.data.variable_infos[0].ty,
        soe_hkx::behavior::VariableType::Int32
    );
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.class == "BSSpeedSamplerModifier"));
    assert!(vm
        .unsupported()
        .unbound
        .iter()
        .any(|binding| binding.contains("speedOut")));
    vm.update(1.0 / 60.0);
    assert_eq!(vm.variables.word(0), Some(17));
}
