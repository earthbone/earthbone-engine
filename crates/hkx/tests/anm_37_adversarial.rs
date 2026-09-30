// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn timer_graph(reset: bool) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(root, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let timer = b.object("BSTimerModifier", 112);
    b.ptr(graph, 128, timer);
    b.set_bool(timer, 72, true);
    b.set_f32(timer, 80, 1.0);
    b.set_i32(timer, 88, 6);
    b.set_bool(timer, 104, reset);
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
fn anm_37_alarm_waits_for_elapsed_time_and_fires_at_boundary() {
    let graph = timer_graph(false);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.4);
    assert!(vm.take_events().is_empty());
    vm.update(0.6);
    assert_eq!(vm.take_events().iter().map(|event| event.id).collect::<Vec<_>>(), [6]);
    vm.update(2.0);
    assert!(vm.take_events().is_empty());
}

#[test]
fn anm_37_reset_alarm_repeats_after_each_full_interval() {
    let graph = timer_graph(true);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(1.0);
    assert_eq!(vm.take_events().len(), 1);
    vm.update(0.5);
    assert!(vm.take_events().is_empty());
    vm.update(0.5);
    assert_eq!(vm.take_events().len(), 1);
}
