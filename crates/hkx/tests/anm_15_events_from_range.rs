// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorGraphData, Binding, EventProperty, EventRange, Node, NodeId, NodeKind,
    VariableBindingSet, VariableInfo, VariableType,
};
use soe_hkx::interp::{ClipLibrary, Event, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn anm_15_decodes_authored_range_events_and_modes() {
    let mut builder = PackfileBuilder::new();
    let container = builder.object("hkRootLevelContainer", 16);
    let graph_object = builder.object("hkbBehaviorGraph", 304);
    let root = builder.object("hkbModifierGenerator", 112);
    let modifier = builder.object("hkbEventsFromRangeModifier", 112);
    let ranges = builder.object("hkbEventRangeDataArray", 32);
    let range_entries = builder.array_structs(ranges, 16, 32, 2);
    let variant = builder.array_structs(container, 0, 24, 1);
    builder.string(variant, 0, "hkbBehaviorGraph");
    builder.string(variant, 8, "hkbBehaviorGraph");
    builder.ptr(variant, 16, graph_object);
    builder.ptr(graph_object, 128, root);
    builder.ptr(root, 72, modifier);
    builder.ptr(modifier, 88, ranges);
    builder.set_bool(modifier, 72, true);
    builder.set_f32(modifier, 80, 0.25);
    builder.set_f32(modifier, 84, 0.0);
    builder.set_f32(range_entries, 0, 1.0);
    builder.set_i32(range_entries, 8, 7);
    builder.set_i8(range_entries, 24, 0);
    builder.set_f32(range_entries + 32, 0, 2.0);
    builder.set_i32(range_entries + 32, 8, 9);
    builder.set_i8(range_entries + 32, 24, 1);

    let bytes = builder.finish();
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let NodeKind::EventsFromRangeModifier {
        input_value,
        lower_bound,
        ranges,
        ..
    } = &graph.node(NodeId(1)).kind
    else {
        panic!("range modifier was not decoded");
    };
    assert_eq!((*input_value, *lower_bound), (0.25, 0.0));
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].0, 1.0);
    assert_eq!(ranges[0].1.event.id, 7);
    assert_eq!(ranges[0].1.event_mode, 0);
    assert_eq!(ranges[1].0, 2.0);
    assert_eq!(ranges[1].1.event.id, 9);
    assert_eq!(ranges[1].1.event_mode, 1);
}

fn graph() -> BehaviorGraph {
    let ranges = vec![
        (
            1.0,
            EventRange {
                event: EventProperty {
                    id: 10,
                    has_payload: false,
                },
                event_mode: 0,
            },
        ),
        (
            2.0,
            EventRange {
                event: EventProperty {
                    id: 20,
                    has_payload: false,
                },
                event_mode: 1,
            },
        ),
    ];
    BehaviorGraph {
        name: "anm_15".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_infos: vec![VariableInfo {
                role: 0,
                role_flags: 0,
                ty: VariableType::Real,
            }],
            variable_names: vec!["Input".into()],
            ..Default::default()
        },
        nodes: vec![
            Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbModifierGenerator".into(),
                name: "Root".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: None,
                },
            },
            Node {
                id: NodeId(1),
                offset: 0,
                class: "hkbEventsFromRangeModifier".into(),
                name: "Ranges".into(),
                user_data: 0,
                bindings: Some(VariableBindingSet {
                    bindings: vec![Binding {
                        member_path: "inputValue".into(),
                        variable_index: 0,
                        bit_index: -1,
                        binding_type: 0,
                    }],
                    index_of_binding_to_enable: -1,
                }),
                kind: NodeKind::EventsFromRangeModifier {
                    enable: true,
                    input_value: 0.0,
                    lower_bound: 0.0,
                    ranges,
                },
            },
        ],
    }
}

#[test]
fn anm_15_crossings_emit_configured_events_and_respect_boundaries() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);

    interpreter.variables.set_by_name("Input", -0.5);
    interpreter.update(0.0);
    assert!(interpreter.take_events().is_empty());

    interpreter.variables.set_by_name("Input", 0.5);
    interpreter.update(0.0);
    assert_eq!(interpreter.take_events(), vec![Event { id: 10 }]);

    // Enter mode sends once for the continuous stay inside [0, 1).
    interpreter.update(0.0);
    assert!(interpreter.take_events().is_empty());

    // The shared boundary belongs to the upper range, which sends each frame.
    interpreter.variables.set_by_name("Input", 1.0);
    interpreter.update(0.0);
    assert_eq!(interpreter.take_events(), vec![Event { id: 20 }]);
    interpreter.update(0.0);
    assert_eq!(interpreter.take_events(), vec![Event { id: 20 }]);

    // The final upper bound is excluded, then re-entering the first range
    // raises its enter event again.
    interpreter.variables.set_by_name("Input", 2.0);
    interpreter.update(0.0);
    assert!(interpreter.take_events().is_empty());
    interpreter.variables.set_by_name("Input", 0.25);
    interpreter.update(0.0);
    assert_eq!(interpreter.take_events(), vec![Event { id: 10 }]);
}
