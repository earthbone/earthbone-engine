// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn graph(maximum: i8, minimum: i8, randomize: bool) -> BehaviorGraph {
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
    b.array_strings(strings, 16, &["input", "output", "other"]);

    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let list = b.object("hkbModifierList", 96);
    b.ptr(wrapper, 72, list);
    b.set_bool(list, 72, true);
    let modifier = b.object("BSEventEveryNEventsModifier", 128);
    b.string(modifier, 56, "every-three");
    b.array_ptrs(list, 80, &[modifier]);
    b.set_bool(modifier, 72, true);
    b.set_i32(modifier, 80, 0);
    b.set_i32(modifier, 96, 1);
    b.set_i8(modifier, 112, maximum);
    b.set_i8(modifier, 113, minimum);
    b.set_bool(modifier, 114, randomize);
    // These are SERIALIZE_IGNORED runtime values and must not seed execution.
    b.set_i32(modifier, 116, 2);
    b.set_i8(modifier, 120, 1);

    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn send(vm: &mut Interpreter<'_>, id: i32) -> Vec<i32> {
    vm.send_event(id);
    vm.update(0.0);
    vm.take_events().into_iter().map(|event| event.id).collect()
}

fn reference(id: NodeId) -> Node {
    Node {
        id,
        offset: 0,
        class: "hkbBehaviorReferenceGenerator".into(),
        name: "behavior-ref".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::BehaviorReference {
            behavior_name: "events".into(),
            resolved: None,
        },
    }
}

#[test]
fn anm_26_authored_interval_emits_once_after_matching_inputs() {
    let graph = graph(3, 1, false);
    let node = graph.find_by_name("every-three").unwrap();
    let NodeKind::EveryNEventsModifier {
        event_to_check_for,
        event_to_send,
        number_of_events_before_send,
        ..
    } = node.kind
    else {
        panic!("modifier should decode its authored event interval")
    };
    assert_eq!(event_to_check_for.id, 0);
    assert_eq!(event_to_send.id, 1);
    assert_eq!(number_of_events_before_send, 3);

    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for _ in 0..2 {
        let delivered = send(&mut vm, 0);
        assert_eq!(delivered.iter().filter(|id| **id == 1).count(), 0);
    }
    let delivered = send(&mut vm, 0);
    assert_eq!(delivered.iter().filter(|id| **id == 1).count(), 1);
    for _ in 0..2 {
        let delivered = send(&mut vm, 0);
        assert_eq!(delivered.iter().filter(|id| **id == 1).count(), 0);
    }
    let delivered = send(&mut vm, 0);
    assert_eq!(delivered.iter().filter(|id| **id == 1).count(), 1);
}

#[test]
fn anm_26_unmatched_inputs_do_not_advance_the_interval() {
    let graph = graph(2, 1, false);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    assert_eq!(send(&mut vm, 2), [2]);
    assert_eq!(send(&mut vm, 0), [0]);
    assert_eq!(send(&mut vm, 2), [2]);
    let delivered = send(&mut vm, 0);
    assert_eq!(delivered.iter().filter(|id| **id == 1).count(), 1);
}

#[test]
fn anm_26_randomized_intervals_stay_within_authored_bounds() {
    let graph = graph(5, 3, true);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    let mut since_output = 0;
    let mut outputs = 0;
    for _ in 0..100 {
        since_output += 1;
        let delivered = send(&mut vm, 0);
        let output_count = delivered.iter().filter(|id| **id == 1).count();
        if output_count == 1 {
            assert!((3..=5).contains(&since_output));
            outputs += 1;
            since_output = 0;
        } else {
            assert_eq!(output_count, 0);
            assert!(since_output < 5);
        }
    }
    assert!(outputs > 0);
}

#[test]
fn anm_26_serialized_runtime_counters_are_ignored() {
    let graph = graph(3, 1, false);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for _ in 0..2 {
        let delivered = send(&mut vm, 0);
        assert!(!delivered.contains(&1));
    }
    assert!(send(&mut vm, 0).contains(&1));
}

#[test]
fn anm_26_linking_remaps_both_authored_event_ids() {
    let subgraph = graph(2, 1, false);
    let mut parent = BehaviorGraph {
        name: "parent".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![reference(NodeId(0))],
    };
    parent.data.event_names = ["output", "other", "input"].map(String::from).to_vec();
    parent.link_reference(NodeId(0), subgraph).unwrap();
    let node = parent.find_by_name("every-three").unwrap();
    let NodeKind::EveryNEventsModifier {
        event_to_check_for,
        event_to_send,
        ..
    } = &node.kind
    else {
        panic!("modifier should retain its authored event properties")
    };
    assert_eq!(event_to_check_for.id, 2);
    assert_eq!(event_to_send.id, 0);

    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&parent, &clips, vec![]);
    let delivered = send(&mut vm, 2);
    assert_eq!(delivered, [2]);
    let delivered = send(&mut vm, 2);
    assert_eq!(delivered.iter().filter(|id| **id == 0).count(), 1);
}
