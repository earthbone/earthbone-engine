// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{BehaviorGraph, Binding, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn fixture() -> BehaviorGraph {
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
    b.array_strings(strings, 16, &["edge1", "edge2", "edge3"]);
    b.array_strings(
        strings,
        48,
        &["input1", "input2", "input3", "enabled", "slot1Enabled"],
    );
    b.array_structs(data, 32, 6, 5);
    b.array_i32(values, 16, &[0, 0, 0, 1, 1]);
    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let list = b.object("hkbModifierList", 96);
    b.ptr(wrapper, 72, list);
    b.set_bool(list, 72, true);
    let modifier = b.object("BSEventOnFalseToTrueModifier", 160);
    b.string(modifier, 56, "edges");
    b.array_ptrs(list, 80, &[modifier]);
    b.set_bool(modifier, 72, true);
    for (slot, offset) in [80, 104, 128].into_iter().enumerate() {
        b.set_bool(modifier, offset, true);
        b.set_bool(modifier, offset + 1, false);
        b.set_i32(modifier, offset + 8, i32::try_from(slot).unwrap());
        // Runtime history must not be restored from ignored bytes.
        b.set_bool(modifier, 152 + u32::try_from(slot).unwrap(), true);
    }
    let bindings = b.object("hkbVariableBindingSet", 40);
    b.ptr(modifier, 16, bindings);
    b.set_i32(bindings, 32, 3);
    let binding = b.array_structs(bindings, 16, 40, 5);
    for (i, path) in [
        "bVariableToTest1",
        "bVariableToTest2",
        "bVariableToTest3",
        "enable",
        "bEnableEvent1",
    ]
    .into_iter()
    .enumerate()
    {
        let at = binding + 40 * u32::try_from(i).unwrap();
        b.string(at, 0, path);
        b.set_i32(at, 28, i32::try_from(i).unwrap());
        b.set_i8(at, 32, -1);
    }
    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn events(vm: &mut Interpreter<'_>) -> Vec<i32> {
    vm.take_events().into_iter().map(|e| e.id).collect()
}

#[test]
fn anm_03_bound_rising_edge_emits_once_and_steady_true_emits_none() {
    let graph = fixture();
    let NodeKind::FalseToTrueModifier { enable, slots } =
        &graph.find_by_name("edges").unwrap().kind
    else {
        panic!("typed modifier")
    };
    assert!(*enable);
    assert_eq!(slots.map(|s| s.event.id), [0, 1, 2]);
    assert!(slots.iter().all(|s| s.enable && !s.variable_to_test));
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    assert!(vm.unsupported().unbound.is_empty());
    assert_eq!(vm.bound_count(), 5);
    vm.update(0.1);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(0, true);
    vm.update(0.1);
    assert_eq!(events(&mut vm), [0]);
    vm.update(0.1);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(0, false);
    vm.update(0.1);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(0, true);
    vm.update(0.1);
    assert_eq!(events(&mut vm), [0]);
}

#[test]
fn all_three_slots_are_independent_and_raise_in_slot_order() {
    let graph = fixture();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for slot in 0..3 {
        vm.variables.set_bool(slot, true);
    }
    vm.update(0.0);
    assert_eq!(events(&mut vm), [0, 1, 2]);
    vm.variables.set_bool(1, false);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
    vm.variables.set_bool(1, true);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [1]);
}

#[test]
fn whole_modifier_and_slot_enable_controls_rearm_after_disable() {
    for enable_index in [3, 4] {
        let graph = fixture();
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.variables.set_bool(0, true);
        vm.variables.set_bool(enable_index, false);
        vm.update(0.0);
        assert!(events(&mut vm).is_empty());
        vm.variables.set_bool(enable_index, true);
        vm.update(0.0);
        assert_eq!(events(&mut vm), [0]);
        vm.variables.set_bool(enable_index, false);
        vm.update(0.0);
        assert!(events(&mut vm).is_empty());
        vm.variables.set_bool(enable_index, true);
        vm.update(0.0);
        assert_eq!(events(&mut vm), [0]);
    }
}

#[test]
fn authored_values_work_without_bindings_and_null_event_ids_are_silent() {
    let mut graph = fixture();
    let id = graph.find_by_name("edges").unwrap().id;
    graph.nodes[id.0].bindings = None;
    if let NodeKind::FalseToTrueModifier { slots, .. } = &mut graph.nodes[id.0].kind {
        for slot in slots.iter_mut() {
            slot.variable_to_test = true;
        }
        slots[1].event.id = -1;
        slots[2].enable = false;
    }
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn serialized_modifier_order_controls_event_order_and_shared_visits_do_not_repeat() {
    let mut graph = fixture();
    let first = graph.find_by_name("edges").unwrap().id;
    let second = NodeId(graph.nodes.len());
    let mut node = graph.nodes[first.0].clone();
    node.id = second;
    if let NodeKind::FalseToTrueModifier { slots, .. } = &mut node.kind {
        slots[0].event.id = 7;
    }
    graph.nodes.push(node);
    let list = graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.kind, NodeKind::ModifierList { .. }))
        .unwrap();
    if let NodeKind::ModifierList { modifiers, .. } = &mut list.kind {
        *modifiers = vec![Some(second), Some(first), Some(second)];
    }
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_bool(0, true);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [7, 0]);
}

#[test]
fn a_failed_list_does_not_emit_an_edge_from_a_partially_visited_modifier() {
    let mut graph = fixture();
    let list = graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.kind, NodeKind::ModifierList { .. }))
        .unwrap();
    let id = list.id;
    if let NodeKind::ModifierList { modifiers, .. } = &mut list.kind {
        modifiers.push(Some(id));
    }
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_bool(0, true);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
    assert_eq!(vm.unsupported().modifier_list_cycles.len(), 1);
}

#[test]
fn malformed_slot_names_are_reported_unbound_without_indexing_outside_the_slots() {
    let mut graph = fixture();
    let id = graph.find_by_name("edges").unwrap().id;
    let bindings = graph.nodes[id.0].bindings.as_mut().unwrap();
    for suffix in ["0", "4", "01", "1/trailing", "99999999999999999999999"] {
        bindings.bindings.push(Binding {
            member_path: format!("bVariableToTest{suffix}"),
            variable_index: 0,
            bit_index: -1,
            binding_type: 0,
        });
    }
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    assert_eq!(vm.unsupported().unbound.len(), 5);
    vm.update(0.0);
}

fn reference(id: NodeId) -> Node {
    Node {
        id,
        offset: 0,
        class: "hkbBehaviorReferenceGenerator".into(),
        name: "reference".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::BehaviorReference {
            behavior_name: "edges".into(),
            resolved: None,
        },
    }
}

#[test]
fn linking_remaps_all_three_slot_events_and_the_bound_variable() {
    let sub = fixture();
    let mut graph = BehaviorGraph {
        name: "parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: Default::default(),
        nodes: vec![reference(NodeId(0))],
    };
    graph.data.event_names = ["other", "edge3", "edge1"].map(String::from).to_vec();
    graph.link_reference(NodeId(0), sub).unwrap();
    let NodeKind::FalseToTrueModifier { slots, .. } = &graph.find_by_name("edges").unwrap().kind
    else {
        panic!("typed modifier")
    };
    assert_eq!(slots.map(|s| s.event.id), [2, 3, 1]);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for name in ["input1", "input2", "input3"] {
        assert!(vm.variables.set_by_name(name, 1.0));
    }
    vm.update(0.0);
    assert_eq!(events(&mut vm), [2, 3, 1]);
}

#[test]
fn the_emitted_edge_reaches_a_parent_state_machine_transition() {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let root = graph.root.unwrap();
    let reference_id = NodeId(graph.nodes.len());
    graph.nodes.push(reference(reference_id));
    let NodeKind::StateMachine(sm) = &mut graph.nodes[root.0].kind else {
        panic!("machine")
    };
    sm.states[0].generator = Some(reference_id);
    let mut sub = fixture();
    sub.data.event_names[0] = "walkStart".into();
    graph.link_reference(reference_id, sub).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_by_name("input1", 1.0);
    vm.update(0.0);
    assert_eq!(vm.current_state(root), Some(1));
    assert_eq!(vm.changes().last().unwrap().event, Some(1));
}

#[test]
fn linking_pads_a_short_parent_default_table_without_losing_existing_scalars() {
    use soe_hkx::behavior::{VariableInfo, VariableType, VariableValueSet};
    let mut graph = BehaviorGraph {
        name: "parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: Default::default(),
        nodes: vec![reference(NodeId(0))],
    };
    graph.data.variable_names = vec!["existing".into(), "missing".into()];
    graph.data.variable_infos = vec![
        VariableInfo {
            ty: VariableType::Int32,
            ..Default::default()
        };
        2
    ];
    graph.data.variable_initial_values = Some(VariableValueSet {
        word_values: vec![7],
        quad_values: vec![],
        variant_values: vec![],
    });
    graph.link_reference(NodeId(0), fixture()).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    assert_eq!(vm.variables.word(0), Some(7));
    assert_eq!(vm.variables.word(1), Some(0));
    assert_eq!(
        vm.variables.word(5),
        Some(1),
        "the appended enable default keeps its variable index"
    );
    vm.variables.set_by_name("input1", 1.0);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [0]);
    assert_eq!(vm.variables.word(0), Some(7));
}

#[test]
fn an_edge_input_can_bind_to_the_character_property_table() {
    use soe_hkx::behavior::{VariableInfo, VariableType};
    let mut graph = fixture();
    graph.data.character_property_names = vec!["property".into()];
    graph.data.character_property_infos = vec![VariableInfo {
        ty: VariableType::Bool,
        ..Default::default()
    }];
    let id = graph.find_by_name("edges").unwrap().id;
    graph.nodes[id.0].bindings.as_mut().unwrap().bindings[0].binding_type = 1;
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    assert!(vm.character_properties.set_bool(0, true));
    assert_eq!(vm.variables.bool(0), Some(false));
    vm.update(0.0);
    assert_eq!(events(&mut vm), [0]);
}
