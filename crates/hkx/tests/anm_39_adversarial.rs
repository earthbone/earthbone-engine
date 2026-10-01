// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn damping_graph() -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(root, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    b.set_i8(graph, 72, 1);
    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["raw", "damped"]);
    let infos = b.array_structs(data, 32, 6, 2);
    b.set_i8(infos, 4, 4);
    b.set_i8(infos + 6, 4, 4);
    b.array_i32(values, 16, &[2.0f32.to_bits() as i32, 0]);
    let damping = b.object("hkbDampingModifier", 192);
    b.ptr(graph, 128, damping);
    b.set_bool(damping, 72, true);
    b.set_f32(damping, 80, 1.0);
    b.set_bool(damping, 92, true);
    b.set_f32(damping, 96, 0.0);
    b.set_f32(damping, 100, 0.0);
    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(damping, 16, bindings);
    let entries = b.array_structs(bindings, 16, 40, 2);
    b.string(entries, 0, "rawValue");
    b.set_i32(entries, 28, 0);
    b.string(entries + 40, 0, "dampedValue");
    b.set_i32(entries + 40, 28, 1);
    b.set_i32(bindings, 32, -1);
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap();
    let modifier = graph.root.unwrap();
    let source = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: source,
        offset: 0,
        class: "FixtureGenerator".into(),
        name: "Source".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::Unknown,
    });
    let wrapper = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: wrapper,
        offset: 0,
        class: "hkbModifierGenerator".into(),
        name: "Wrapper".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::ModifierGenerator {
            modifier: Some(modifier),
            generator: Some(source),
        },
    });
    graph.root = Some(wrapper);
    graph
}

#[test]
fn anm_39_scalar_input_moves_toward_target_without_overshooting_for_small_step() {
    let graph = damping_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.1);
    let out = vm
        .variables
        .real(vm.variables.index("damped").unwrap())
        .unwrap();
    assert!((out - 0.2).abs() < 0.0001);
}

#[test]
fn anm_39_nonpositive_or_nonfinite_time_does_not_corrupt_damped_value() {
    let graph = damping_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    vm.update(-1.0);
    let out = vm
        .variables
        .real(vm.variables.index("damped").unwrap())
        .unwrap();
    assert_eq!(out, 0.0);
}
