// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn graph(direction: [f32; 4], radius: f32) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let modifier = b.object("BSPassByTargetTriggerModifier", 160);
    b.ptr(wrapper, 72, modifier);
    b.set_bool(modifier, 72, true);
    b.set_vec4(modifier, 80, [1.0, 2.0, 3.0, 0.0]);
    b.set_f32(modifier, 96, radius);
    b.set_vec4(modifier, 112, direction);
    b.set_i32(modifier, 128, 7);
    // Runtime history is ignored when a graph is loaded.
    b.set_bool(modifier, 144, true);
    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn events(vm: &mut Interpreter<'_>) -> Vec<i32> {
    vm.take_events().into_iter().map(|event| event.id).collect()
}

fn reference() -> Node {
    Node {
        id: NodeId(0),
        offset: 0,
        class: "hkbBehaviorReferenceGenerator".into(),
        name: "reference".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::BehaviorReference {
            behavior_name: "pass_by_target".into(),
            resolved: None,
        },
    }
}

#[test]
fn anm_34_decodes_target_radius_direction_and_event() {
    let graph = graph([1.0, 0.0, 0.0, 0.0], 0.5);
    let NodeKind::PassByTargetTrigger { enable, trigger } = &graph.nodes[1].kind else {
        panic!("typed pass-by-target modifier")
    };
    assert!(*enable);
    assert_eq!(trigger.target_position, [1.0, 2.0, 3.0, 0.0]);
    assert_eq!(trigger.radius, 0.5);
    assert_eq!(trigger.movement_direction, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(trigger.trigger_event.id, 7);
    assert!(!trigger.trigger_event.has_payload);
}

#[test]
fn anm_34_crossing_target_boundary_emits_configured_event_once() {
    let graph = graph([1.0, 0.0, 0.0, 0.0], 0.5);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [7]);
    vm.set_character_position([3.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn anm_34_requires_forward_crossing_inside_target_radius() {
    let graph = graph([1.0, 0.0, 0.0, 0.0], 0.5);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
    vm.set_character_position([0.0, 3.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([2.0, 3.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([0.0, 3.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn anm_34_invalid_direction_is_silent() {
    let graph = graph([0.0, 0.0, 0.0, 0.0], 0.5);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
}

#[test]
fn anm_34_linked_trigger_event_uses_parent_event_id() {
    let mut sub = graph([1.0, 0.0, 0.0, 0.0], 0.5);
    sub.data.event_names.push("targetCrossed".into());
    if let NodeKind::PassByTargetTrigger { trigger, .. } = &mut sub.nodes[1].kind {
        trigger.trigger_event.id = 0;
    }
    let mut parent = BehaviorGraph {
        name: "parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![reference()],
    };
    parent.data.event_names = ["other", "targetCrossed"].map(String::from).to_vec();
    parent.link_reference(NodeId(0), sub).unwrap();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&parent, &clips, vec![]);
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    assert_eq!(events(&mut vm), [1]);
}

#[test]
fn anm_34_disabled_modifier_does_not_emit() {
    let mut graph = graph([1.0, 0.0, 0.0, 0.0], 0.5);
    if let NodeKind::PassByTargetTrigger { enable, .. } = &mut graph.nodes[1].kind {
        *enable = false;
    }
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.set_character_position([0.0, 2.0, 3.0]);
    vm.update(0.0);
    vm.set_character_position([2.0, 2.0, 3.0]);
    vm.update(0.0);
    assert!(events(&mut vm).is_empty());
}
