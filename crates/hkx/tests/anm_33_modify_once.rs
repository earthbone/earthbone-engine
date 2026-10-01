// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn fixture(enabled: bool) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 16, &["enabled"]);
    b.array_strings(strings, 48, &["enabled"]);
    b.array_structs(data, 32, 6, 1);
    b.array_i32(values, 16, &[i32::from(enabled)]);

    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let modify_once = b.object("BSModifyOnceModifier", 112);
    b.ptr(wrapper, 72, modify_once);
    b.set_bool(modify_once, 72, true);
    let on_activate = b.object("BSIsActiveModifier", 96);
    let on_deactivate = b.object("BSIsActiveModifier", 96);
    b.string(on_activate, 56, "on_activate");
    b.string(on_deactivate, 56, "on_deactivate");
    b.ptr(modify_once, 80, on_activate);
    b.ptr(modify_once, 96, on_deactivate);
    b.set_bool(on_activate, 72, true);
    b.set_bool(on_deactivate, 72, true);
    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(modify_once, 16, bindings);
    b.set_i32(bindings, 32, 1);
    let binding = b.array_structs(bindings, 16, 40, 1);
    b.string(binding, 0, "enable");
    b.set_i32(binding, 28, 0);
    b.set_i8(binding, 32, -1);

    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

#[test]
fn anm_33_activation_runs_its_declared_modifier_once() {
    let graph = fixture(true);
    let modify_once = graph
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::ModifyOnce { .. }))
        .unwrap();
    let NodeKind::ModifyOnce {
        on_activate,
        on_deactivate,
        ..
    } = &modify_once.kind
    else {
        panic!("decoded BSModifyOnceModifier")
    };
    let activate_id = on_activate.unwrap();
    let deactivate_id = on_deactivate.unwrap();
    assert!(matches!(
        graph.node(activate_id).kind,
        NodeKind::IsActiveModifier { .. }
    ));
    assert!(matches!(
        graph.node(deactivate_id).kind,
        NodeKind::IsActiveModifier { .. }
    ));
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.1);
    assert!(vm.is_active(activate_id));
    assert!(!vm.is_active(deactivate_id));
    vm.update(0.1);
    assert!(vm.is_active(activate_id));
    assert!(!vm.is_active(deactivate_id));
}

#[test]
fn anm_33_deactivation_runs_its_declared_modifier_once() {
    let graph = fixture(true);
    let modify_once = graph
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::ModifyOnce { .. }))
        .unwrap();
    let NodeKind::ModifyOnce { on_deactivate, .. } = modify_once.kind else {
        panic!("decoded BSModifyOnceModifier")
    };
    let child = on_deactivate.unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.1);
    assert!(!vm.is_active(child));
    vm.variables.set_bool(0, false);
    vm.update(0.1);
    assert!(!vm.is_active(modify_once.id));
    assert!(!vm.is_active(child));
    vm.update(0.1);
    assert!(!vm.is_active(child));
}

#[test]
fn anm_33_disabled_parent_does_not_activate_either_child() {
    let graph = fixture(false);
    let modify_once = graph
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::ModifyOnce { .. }))
        .unwrap();
    let NodeKind::ModifyOnce {
        on_activate,
        on_deactivate,
        ..
    } = &modify_once.kind
    else {
        panic!("decoded BSModifyOnceModifier")
    };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.update(0.1);
    assert!(!vm.is_active(modify_once.id));
    assert!(!vm.is_active(on_activate.unwrap()));
    assert!(!vm.is_active(on_deactivate.unwrap()));
}
