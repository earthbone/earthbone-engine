// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Transition guards on a binary fixture and on linked graphs. Layouts follow
//! serde-hkx's hkbExpressionCondition and hkbStateMachineTransitionInfo.

mod common;

use soe_hkx::behavior::{
    transition_flags, BehaviorGraph, BehaviorGraphData, Condition, Node, NodeId, NodeKind,
    VariableInfo, VariableType,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn guarded(source: &str, event: i32) -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    graph.data.variable_names.push("Enabled".into());
    graph.data.variable_infos.push(VariableInfo {
        ty: VariableType::Bool,
        ..Default::default()
    });
    let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("machine");
    };
    sm.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    sm.states[0].transitions[0].event_id = event;
    sm.states[0].transitions[0].transition = None;
    graph
}

fn current(vm: &Interpreter<'_>) -> i32 {
    vm.current_state(vm.active_states()[0].0).unwrap()
}

#[test]
fn anm_01_false_guard_blocks_until_typed_variables_select_branch() {
    let graph = guarded("Enabled && Speed > 1.5 && iState == 3", 1);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    for (speed, state, enabled, expected) in [
        (2.0, 3, false, 0),
        (1.0, 3, true, 0),
        (2.0, 2, true, 0),
        (2.0, 3, true, 1),
    ] {
        vm.variables.set_real(0, speed);
        vm.variables.set_word(1, state);
        vm.variables.set_bool(2, enabled);
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(current(&vm), expected);
    }
    assert!(vm.unsupported().conditions.is_empty());
}

#[test]
fn condition_only_transition_is_polled_without_an_event() {
    let graph = guarded("Speed > 1.5", -1);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.update(0.0);
    assert_eq!(current(&vm), 0);
    vm.variables.set_real(0, 2.0);
    vm.update(0.0);
    assert_eq!(current(&vm), 1);
}

#[test]
fn anm_49_authored_math_functions_select_a_guard_branch() {
    let graph = guarded(
        "cond(clamp(pow(2, 3), 0, 9) == 8 && sind(30) == .5, 1, 0) > 0",
        1,
    );
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    assert_eq!(current(&vm), 1);
    assert!(vm.unsupported().conditions.is_empty());
}

#[test]
fn anm_49_random_guard_replays_from_the_interpreter_seed() {
    let mut graph = guarded("randrange(-1, 1) < 0", 1);
    let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("machine");
    };
    let mut back = sm.states[0].transitions[0].clone();
    back.to_state_id = 0;
    back.event_id = -1;
    sm.states[1].transitions.push(back);
    let clips = ClipLibrary::new();
    let mut first = Interpreter::new(&graph, &clips, Vec::new());
    let mut replay = Interpreter::new(&graph, &clips, Vec::new());
    for _ in 0..8 {
        first.send_event(1);
        replay.send_event(1);
        first.update(0.0);
        replay.update(0.0);
        assert_eq!(current(&first), current(&replay));
    }
    assert!(first.unsupported().conditions.is_empty());
    assert!(replay.unsupported().conditions.is_empty());
}

#[test]
fn unknown_and_malformed_guards_block_and_report_once() {
    let clips = ClipLibrary::new();
    for condition in [
        Condition::Unsupported("hkbScriptCondition".into()),
        Condition::Expression("Missing == 0".into()),
        Condition::Expression("Speed >".into()),
        Condition::Expression("1 / 0".into()),
    ] {
        let mut graph = guarded("true", 1);
        let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
            panic!("machine");
        };
        sm.states[0].transitions[0].condition = Some(condition);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        for _ in 0..3 {
            vm.send_event(1);
            vm.update(0.0);
        }
        assert_eq!(current(&vm), 0);
        assert_eq!(vm.unsupported().conditions.len(), 1);
    }
}

#[test]
fn disabled_condition_bypasses_guard_but_disabled_transition_does_not() {
    let clips = ClipLibrary::new();
    for (flags, expected) in [
        (transition_flags::DISABLE_CONDITION, 1),
        (
            transition_flags::DISABLE_CONDITION | transition_flags::DISABLED,
            0,
        ),
    ] {
        let mut graph = guarded("Missing", 1);
        let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
            panic!("machine");
        };
        sm.states[0].transitions[0].flags = flags;
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(current(&vm), expected);
        assert!(vm.unsupported().conditions.is_empty());
    }
}

#[test]
fn linked_guard_uses_parent_variable_ids_and_remapped_event_id() {
    let mut graph = BehaviorGraph {
        name: "parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: vec!["iState".into(), "Unrelated".into(), "Speed".into()],
            variable_infos: vec![
                VariableInfo {
                    ty: VariableType::Int32,
                    ..Default::default()
                },
                VariableInfo::default(),
                VariableInfo {
                    ty: VariableType::Real,
                    ..Default::default()
                },
            ],
            event_names: vec!["walkStart".into(), "other".into(), "idleStart".into()],
            ..Default::default()
        },
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbBehaviorReferenceGenerator".into(),
            name: "reference".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::BehaviorReference {
                behavior_name: "sub.hkx".into(),
                resolved: None,
            },
        }],
    };
    graph
        .link_reference(NodeId(0), guarded("Speed > 1.5 && iState == 3", 1))
        .unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.variables.set_by_name("Speed", 2.0);
    vm.variables.set_by_name("iState", 3.0);
    vm.send_event(0);
    vm.update(0.0);
    assert_eq!(current(&vm), 1);
}

#[test]
fn condition_cycle_advances_at_most_once_per_frame() {
    let mut graph = guarded("true", -1);
    let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("machine");
    };
    let mut back = sm.states[0].transitions[0].clone();
    back.to_state_id = 0;
    sm.states[1].transitions.push(back);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    for expected in [1, 0, 1, 0] {
        vm.update(0.0);
        assert_eq!(current(&vm), expected);
    }
}

#[test]
fn binary_condition_decodes_expression_without_node_header() {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let data = b.object("hkbBehaviorGraphData", 128);
    b.ptr(graph, 136, data);
    let sm = b.object("hkbStateMachine", 264);
    b.ptr(graph, 128, sm);
    let state = b.object("hkbStateMachineStateInfo", 120);
    b.array_ptrs(sm, 144, &[state]);
    let transitions = b.object("hkbStateMachineTransitionInfoArray", 32);
    b.ptr(state, 80, transitions);
    let transition = b.array_structs(transitions, 16, 72, 1);
    let condition = b.object("hkbExpressionCondition", 32);
    b.ptr(transition, 40, condition);
    b.string(condition, 16, "Speed > 0.5");
    let bytes = b.finish();
    let graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let NodeKind::StateMachine(sm) = &graph.nodes[graph.root.unwrap().0].kind else {
        panic!("machine");
    };
    assert_eq!(
        sm.states[0].transitions[0].condition,
        Some(Condition::Expression("Speed > 0.5".into()))
    );
}

fn reference_parent(data: BehaviorGraphData) -> BehaviorGraph {
    BehaviorGraph {
        name: "property parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data,
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbBehaviorReferenceGenerator".into(),
            name: "property reference".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::BehaviorReference {
                behavior_name: "child.hkx".into(),
                resolved: None,
            },
        }],
    }
}

fn property_guarded() -> BehaviorGraph {
    let mut graph = guarded("Gate > 1.5", 1);
    graph.data.character_property_names = vec!["Shared".into(), "Gate".into()];
    graph.data.character_property_infos = vec![
        VariableInfo::default(),
        VariableInfo {
            ty: VariableType::Real,
            ..Default::default()
        },
    ];
    let walk = graph
        .nodes
        .iter_mut()
        .find(|node| node.name == "WalkClip")
        .unwrap();
    let binding = &mut walk.bindings.as_mut().unwrap().bindings[0];
    binding.binding_type = 1;
    binding.variable_index = 1;
    graph
}

#[test]
fn linked_child_property_guard_matches_standalone_and_remaps_property_bindings() {
    let standalone = property_guarded();
    let mut linked = reference_parent(BehaviorGraphData {
        variable_names: vec!["Gate".into()],
        variable_infos: vec![VariableInfo::default()],
        character_property_names: vec!["Other".into(), "Shared".into()],
        character_property_infos: vec![VariableInfo::default(); 2],
        ..Default::default()
    });
    linked
        .link_reference(NodeId(0), property_guarded())
        .unwrap();
    assert_eq!(
        linked.data.character_property_names,
        ["Other", "Shared", "Gate"]
    );
    assert_eq!(
        linked.data.character_property_infos[2].ty,
        VariableType::Real
    );
    let walk = linked
        .nodes
        .iter()
        .find(|node| node.name == "WalkClip")
        .unwrap();
    assert_eq!(
        walk.bindings.as_ref().unwrap().bindings[0].variable_index,
        2
    );
    let clips = ClipLibrary::new();
    for graph in [&standalone, &linked] {
        let mut vm = Interpreter::new(graph, &clips, Vec::new());
        assert!(vm.unsupported().conditions.is_empty());
        vm.variables.set_by_name("Gate", 0.0);
        vm.character_properties.set_by_name("Gate", 2.0);
        vm.send_event_named("walkStart");
        vm.update(0.0);
        assert_eq!(current(&vm), 1);
    }
}

#[test]
fn parent_property_and_child_variable_keep_distinct_scopes_for_identical_source() {
    let mut parent = property_guarded();
    let parent_root = parent.root.unwrap();
    let mut child = guarded("Gate > 1.5", 1);
    child.data.variable_names.push("Gate".into());
    child.data.variable_infos.push(VariableInfo {
        ty: VariableType::Real,
        ..Default::default()
    });
    let reference = NodeId(parent.nodes.len());
    let mut reference_node = reference_parent(BehaviorGraphData::default())
        .nodes
        .remove(0);
    reference_node.id = reference;
    parent.nodes.push(reference_node);
    let child_root = parent.link_reference(reference, child).unwrap();
    let clips = ClipLibrary::new();
    for (root, variable, property, expected) in [
        (parent_root, 0.0, 2.0, 1),
        (parent_root, 2.0, 0.0, 0),
        (child_root, 0.0, 2.0, 0),
        (child_root, 2.0, 0.0, 1),
    ] {
        parent.root = Some(root);
        let mut vm = Interpreter::new(&parent, &clips, Vec::new());
        vm.variables.set_by_name("Gate", variable);
        vm.character_properties.set_by_name("Gate", property);
        vm.send_event_named("walkStart");
        vm.update(0.0);
        assert_eq!(current(&vm), expected);
    }
}

#[test]
fn expression_scope_and_property_bindings_survive_a_second_link() {
    let mut middle = reference_parent(BehaviorGraphData::default());
    middle
        .link_reference(NodeId(0), property_guarded())
        .unwrap();
    let mut outer = reference_parent(BehaviorGraphData {
        variable_names: vec!["Gate".into()],
        variable_infos: vec![VariableInfo::default()],
        character_property_names: vec!["Other".into()],
        character_property_infos: vec![VariableInfo::default()],
        ..Default::default()
    });
    outer.link_reference(NodeId(0), middle).unwrap();
    let walk = outer
        .nodes
        .iter()
        .find(|node| node.name == "WalkClip")
        .unwrap();
    assert_eq!(
        walk.bindings.as_ref().unwrap().bindings[0].variable_index,
        2
    );
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&outer, &clips, Vec::new());
    vm.character_properties.set_by_name("Gate", 2.0);
    vm.send_event_named("walkStart");
    vm.update(0.0);
    assert_eq!(current(&vm), 1);
}

#[test]
fn external_event_can_enter_a_state_whose_guard_runs_in_the_same_frame() {
    let mut graph = guarded("true", 1);
    let NodeKind::StateMachine(sm) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("machine");
    };
    let mut back = sm.states[0].transitions[0].clone();
    back.event_id = -1;
    back.to_state_id = 0;
    sm.states[1].transitions.push(back);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    let changes = vm.take_changes();
    assert_eq!(
        changes
            .iter()
            .map(|change| (change.to, change.event))
            .collect::<Vec<_>>(),
        [(0, None), (1, Some(1)), (0, None)]
    );
}
