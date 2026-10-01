// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn node(id: usize, name: &str, kind: NodeKind) -> Node {
    Node {
        id: NodeId(id),
        offset: id as u32,
        class: name.into(),
        name: name.into(),
        user_data: 0,
        bindings: None,
        kind,
    }
}

fn graph(active_by_default: bool) -> BehaviorGraph {
    BehaviorGraph {
        name: "event-driven-test".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            event_names: vec!["Activate".into(), "Deactivate".into(), "Other".into()],
            ..Default::default()
        },
        nodes: vec![
            node(
                0,
                "hkbModifierGenerator",
                NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: None,
                },
            ),
            node(
                1,
                "hkbEventDrivenModifier",
                NodeKind::EventDrivenModifier {
                    enable: true,
                    modifier: Some(NodeId(2)),
                    activate_event_id: 0,
                    deactivate_event_id: 1,
                    active_by_default,
                },
            ),
            node(2, "hkbTestModifier", NodeKind::Modifier { enable: true }),
        ],
    }
}

#[test]
fn anm_17_activate_and_deactivate_events_gate_the_child_interval() {
    let graph = graph(false);
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(1)));
    assert!(!interpreter.is_active(NodeId(2)));

    interpreter.send_event_named("Other");
    interpreter.update(0.0);
    assert!(!interpreter.is_active(NodeId(2)));

    interpreter.send_event_named("Activate");
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(2)));

    interpreter.send_event_named("Deactivate");
    interpreter.update(0.0);
    assert!(!interpreter.is_active(NodeId(2)));
}

#[test]
fn anm_17_active_by_default_and_wrapper_enable_control_child_state() {
    let default_graph = graph(true);
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&default_graph, &clips, Vec::new());
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(2)));

    interpreter.send_event_named("Deactivate");
    interpreter.update(0.0);
    assert!(!interpreter.is_active(NodeId(2)));
    interpreter.send_event_named("Activate");
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(2)));

    let mut disabled_graph = graph(false);
    let NodeKind::EventDrivenModifier {
        enable: wrapper_enable,
        ..
    } = &mut disabled_graph.nodes[1].kind
    else {
        panic!("event-driven modifier");
    };
    *wrapper_enable = false;
    let disabled_clips = ClipLibrary::new();
    let mut disabled = Interpreter::new(&disabled_graph, &disabled_clips, Vec::new());
    disabled.update(0.0);
    assert!(!disabled.is_active(NodeId(1)));
    assert!(!disabled.is_active(NodeId(2)));
    disabled.send_event_named("Activate");
    disabled.update(0.0);
    assert!(!disabled.is_active(NodeId(2)));
}

#[test]
fn anm_17_other_events_reach_an_active_child_modifier() {
    let mut graph = graph(false);
    graph.data.event_names.push("ChildActivate".into());
    graph.nodes[2].kind = NodeKind::EventDrivenModifier {
        enable: true,
        modifier: Some(NodeId(3)),
        activate_event_id: 3,
        deactivate_event_id: -1,
        active_by_default: false,
    };
    graph.nodes.push(node(
        3,
        "hkbChildModifier",
        NodeKind::Modifier { enable: true },
    ));
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());

    interpreter.send_event_named("Activate");
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(2)));
    assert!(!interpreter.is_active(NodeId(3)));

    interpreter.send_event_named("ChildActivate");
    interpreter.update(0.0);
    assert!(interpreter.is_active(NodeId(3)));
}

#[test]
fn anm_17_packfile_decodes_event_ids_default_and_modifier_child() {
    let mut builder = PackfileBuilder::new();
    let container = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 304);
    let variant = builder.array_structs(container, 0, 24, 1);
    builder.string(variant, 0, "hkbBehaviorGraph");
    builder.string(variant, 8, "hkbBehaviorGraph");
    builder.ptr(variant, 16, graph);
    let wrapper = builder.object("hkbEventDrivenModifier", 104);
    builder.ptr(graph, 128, wrapper);
    builder.string(wrapper, 56, "EventGate");
    builder.set_bool(wrapper, 72, true);
    builder.set_i32(wrapper, 88, 7);
    builder.set_i32(wrapper, 92, 9);
    builder.set_bool(wrapper, 96, true);
    builder.set_bool(wrapper, 97, false);
    let child = builder.object("hkbTestModifier", 80);
    builder.ptr(wrapper, 80, child);
    builder.set_bool(child, 72, true);

    let packfile = Packfile::parse(&builder.finish()).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let NodeKind::EventDrivenModifier {
        enable,
        modifier,
        activate_event_id,
        deactivate_event_id,
        active_by_default,
    } = &graph.node(NodeId(0)).kind
    else {
        panic!("event-driven modifier was not decoded");
    };
    assert!(*enable);
    assert_eq!(*modifier, Some(NodeId(1)));
    assert_eq!((*activate_event_id, *deactivate_event_id), (7, 9));
    assert!(*active_by_default);
    assert!(matches!(
        graph.node(NodeId(1)).kind,
        NodeKind::Modifier { enable: true }
    ));
}
