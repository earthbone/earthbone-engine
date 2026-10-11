// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-36 speed sampler behavior in a synthetic behavior graph.

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn speed_sampler_graph(enabled: bool, initial_speed_out: f32) -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    b.string(graph, 56, "speed sampler test");
    b.set_i8(graph, 72, 1);

    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(
        strings,
        48,
        &["state", "direction", "goalSpeed", "speedOut"],
    );
    let infos = b.array_structs(data, 32, 6, 4);
    b.set_i8(infos, 4, 3);
    b.set_i8(infos + 6, 4, 4);
    b.set_i8(infos + 12, 4, 4);
    b.set_i8(infos + 18, 4, 4);
    b.array_i32(
        values,
        16,
        &[
            1,
            0.5f32.to_bits() as i32,
            8.0f32.to_bits() as i32,
            initial_speed_out.to_bits() as i32,
        ],
    );

    let root = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, root);
    b.string(root, 56, "root");
    let sampler = b.object("BSSpeedSamplerModifier", 96);
    b.string(sampler, 56, "speed sampler");
    b.set_bool(sampler, 72, enabled);
    b.set_i32(sampler, 80, -1);
    b.set_f32(sampler, 84, 0.0);
    b.set_f32(sampler, 88, 0.0);
    b.set_f32(sampler, 92, 0.0);
    b.ptr(root, 72, sampler);

    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(sampler, 16, bindings);
    let bind = b.array_structs(bindings, 16, 40, 4);
    for (offset, (path, index)) in [
        ("state", 0),
        ("direction", 1),
        ("goalSpeed", 2),
        ("speedOut", 3),
    ]
    .into_iter()
    .enumerate()
    {
        let item = bind + (offset as u32) * 40;
        b.string(item, 0, path);
        b.set_i32(item, 28, index);
    }
    b.set_i32(bindings, 32, -1);
    b.finish()
}

fn interpreter(bytes: &[u8]) -> (BehaviorGraph, ClipLibrary, Vec<QsTransform>) {
    let packfile = Packfile::parse(bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let clips = ClipLibrary::new();
    let pose = vec![QsTransform::IDENTITY];
    (graph, clips, pose)
}

#[test]
fn anm_36_speed_sampler_decodes_fields_and_publishes_goal_speed() {
    let bytes = speed_sampler_graph(true, 0.0);
    let (graph, clips, pose) = interpreter(&bytes);
    let sampler = graph
        .nodes
        .iter()
        .find(|node| node.class == "BSSpeedSamplerModifier")
        .unwrap();
    assert!(matches!(
        sampler.kind,
        NodeKind::SpeedSampler {
            enable: true,
            state: -1,
            direction,
            goal_speed: 0.0,
            speed_out: 0.0,
        } if direction == 0.0
    ));

    let mut vm = Interpreter::new(&graph, &clips, pose);
    assert!(vm.unsupported().unbound.is_empty());
    assert!(vm.variables.set_by_name("state", 2.0));
    assert!(vm.variables.set_by_name("direction", -1.0));
    assert!(vm.variables.set_by_name("goalSpeed", 123.5));
    vm.update(1.0 / 60.0);
    let output = vm.variables.real(vm.variables.index("speedOut").unwrap());
    assert_eq!(output, Some(123.5));
}

#[test]
fn anm_36_disabled_speed_sampler_preserves_the_output_binding() {
    let bytes = speed_sampler_graph(false, 19.0);
    let (graph, clips, pose) = interpreter(&bytes);
    let mut vm = Interpreter::new(&graph, &clips, pose);
    vm.variables.set_by_name("goalSpeed", 123.5);
    vm.update(1.0 / 60.0);
    let output = vm.variables.real(vm.variables.index("speedOut").unwrap());
    assert_eq!(output, Some(19.0));
}

#[test]
fn anm_36_state_and_direction_bindings_preserve_goal_speed_output() {
    for (state, direction, goal_speed) in [
        (-1.0, -1.0, 123.5),
        (0.0, 0.0, 0.0),
        (2.0, 1.0, -7.25),
        (9.0, -0.5, 42.0),
    ] {
        let bytes = speed_sampler_graph(true, 0.0);
        let (graph, clips, pose) = interpreter(&bytes);
        let mut vm = Interpreter::new(&graph, &clips, pose);
        assert!(vm.unsupported().unbound.is_empty());
        assert!(vm.variables.set_by_name("state", state));
        assert!(vm.variables.set_by_name("direction", direction));
        assert!(vm.variables.set_by_name("goalSpeed", goal_speed));

        vm.update(1.0 / 60.0);

        let speed_out = vm.variables.real(vm.variables.index("speedOut").unwrap());
        assert_eq!(
            speed_out,
            Some(goal_speed),
            "state={state}, direction={direction}"
        );
    }
}

#[test]
fn anm_36_context_types_and_goal_speed_publish_to_speed_out() {
    let bytes = speed_sampler_graph(true, 0.0);
    let (graph, clips, pose) = interpreter(&bytes);
    assert_eq!(
        graph.data.variable_infos[0].ty,
        soe_hkx::behavior::VariableType::Int32
    );
    assert_eq!(
        graph.data.variable_infos[1].ty,
        soe_hkx::behavior::VariableType::Real
    );
    assert_eq!(
        graph.data.variable_infos[2].ty,
        soe_hkx::behavior::VariableType::Real
    );
    assert_eq!(
        graph.data.variable_infos[3].ty,
        soe_hkx::behavior::VariableType::Real
    );

    let mut vm = Interpreter::new(&graph, &clips, pose);
    assert!(vm.unsupported().unbound.is_empty());
    let state = vm.variables.index("state").unwrap();
    let direction = vm.variables.index("direction").unwrap();
    let goal_speed = vm.variables.index("goalSpeed").unwrap();
    let speed_out = vm.variables.index("speedOut").unwrap();

    for (state_value, direction_value, speed_value) in [
        (i32::MIN, -1.0, 0.0),
        (0, 0.0, 37.25),
        (i32::MAX, 1.0, -8.5),
    ] {
        assert!(vm.variables.set_word(state, state_value));
        assert!(vm.variables.set_real(direction, direction_value));
        assert!(vm.variables.set_real(goal_speed, speed_value));
        vm.update(1.0 / 60.0);
        assert_eq!(vm.variables.word(state), Some(state_value));
        assert_eq!(vm.variables.real(direction), Some(direction_value));
        assert_eq!(vm.variables.real(speed_out), Some(speed_value));
    }
}
