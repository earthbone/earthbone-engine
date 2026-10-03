// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial ANM-36 checks for BSSpeedSamplerModifier output.

use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn graph(bind_goal_speed: bool, authored_goal_speed: f32) -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    b.string(graph, 56, "speed sampler adversarial test");
    b.set_i8(graph, 72, 1);

    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["state", "direction", "goalSpeed", "speedOut"]);
    let infos = b.array_structs(data, 32, 6, 4);
    b.set_i8(infos, 4, 3);
    for offset in [6, 12, 18] {
        b.set_i8(infos + offset, 4, 4);
    }
    b.array_i32(
        values,
        16,
        &[
            1,
            0.5f32.to_bits() as i32,
            authored_goal_speed.to_bits() as i32,
            0.0f32.to_bits() as i32,
        ],
    );

    let root = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, root);
    b.string(root, 56, "root");
    let sampler = b.object("BSSpeedSamplerModifier", 96);
    b.string(sampler, 56, "speed sampler");
    b.set_bool(sampler, 72, true);
    b.set_i32(sampler, 80, -1);
    b.set_f32(sampler, 84, 0.0);
    b.set_f32(sampler, 88, authored_goal_speed);
    b.set_f32(sampler, 92, 0.0);
    b.ptr(root, 72, sampler);

    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(sampler, 16, bindings);
    let mut entries = vec![("state", 0), ("direction", 1)];
    if bind_goal_speed {
        entries.push(("goalSpeed", 2));
    }
    entries.push(("speedOut", 3));
    let bind = b.array_structs(bindings, 16, 40, entries.len());
    for (offset, (path, index)) in entries.into_iter().enumerate() {
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
    (graph, ClipLibrary::new(), vec![QsTransform::IDENTITY])
}

#[test]
fn anm_36_sampler_publishes_latest_goal_speed_on_repeated_updates() {
    let bytes = graph(true, 8.0);
    let (graph, clips, pose) = interpreter(&bytes);
    let mut vm = Interpreter::new(&graph, &clips, pose);

    for (state, direction, goal_speed) in [(0.0, -1.0, 0.0), (3.0, 1.0, -12.75), (-2.0, 0.25, 327.5)] {
        assert!(vm.variables.set_by_name("state", state));
        assert!(vm.variables.set_by_name("direction", direction));
        assert!(vm.variables.set_by_name("goalSpeed", goal_speed));
        vm.update(1.0 / 60.0);
        let output = vm.variables.real(vm.variables.index("speedOut").unwrap());
        assert_eq!(output, Some(goal_speed), "state={state}, direction={direction}");
    }
}

#[test]
fn anm_36_sampler_uses_authored_goal_when_goal_binding_is_absent() {
    let bytes = graph(false, 42.25);
    let (graph, clips, pose) = interpreter(&bytes);
    let mut vm = Interpreter::new(&graph, &clips, pose);
    assert!(vm.unsupported().unbound.is_empty());

    vm.update(1.0 / 60.0);

    let output = vm.variables.real(vm.variables.index("speedOut").unwrap());
    assert_eq!(output, Some(42.25));
}

#[test]
fn anm_36_sampler_publishes_authored_zero_without_a_goal_binding() {
    let bytes = graph(false, 0.0);
    let (graph, clips, pose) = interpreter(&bytes);
    let mut vm = Interpreter::new(&graph, &clips, pose);

    vm.update(1.0 / 60.0);

    let output = vm.variables.real(vm.variables.index("speedOut").unwrap());
    assert_eq!(output, Some(0.0));
}
