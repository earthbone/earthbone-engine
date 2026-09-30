// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{graph, Root};
use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Event, Interpreter};
use soe_hkx::Packfile;

#[test]
fn anm_27_deactivation_emits_authored_event_once() {
    let bytes = graph(&Root::Deactivate, "mt_idle", "mt_walkforward");
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let event_modifier = graph
        .nodes
        .iter()
        .find(|node| node.class == "BSEventOnDeactivateModifier")
        .expect("deactivation modifier is decoded");
    assert!(matches!(
        event_modifier.kind,
        NodeKind::EventOnDeactivateModifier {
            enable: true,
            event: soe_hkx::behavior::EventProperty { id: 2, .. }
        }
    ));

    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, Vec::new());
    interpreter.update(1.0 / 60.0);
    assert!(interpreter.variables.set_word(2, 0));
    interpreter.update(1.0 / 60.0);
    assert_eq!(interpreter.take_events(), vec![Event { id: 2 }]);

    interpreter.update(1.0 / 60.0);
    assert!(interpreter.take_events().is_empty());
}
