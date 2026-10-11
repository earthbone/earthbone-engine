// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{Condition, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn guarded(source: &str) -> soe_hkx::behavior::BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let mut graph = soe_hkx::behavior::BehaviorGraph::from_packfile(
        &Packfile::parse(&bytes).expect("fixture parses"),
    )
    .expect("fixture graph decodes");
    let NodeKind::StateMachine(machine) = &mut graph.nodes[graph.root.expect("root").0].kind
    else {
        panic!("fixture root is not a state machine");
    };
    machine.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    machine.states[0].transitions[0].event_id = 1;
    machine.states[0].transitions[0].transition = None;
    graph
}

#[test]
fn inverse_trig_matches_havok_domain_edges_for_both_signs_and_units() {
    let clips = ClipLibrary::new();
    for source in [
        "asin(1) > 1.57",
        "asin(-1) < -1.57",
        "acos(1) == 0",
        "acos(-1) > 3.14",
        "asind(1.00099) == 90",
        "asind(-1.00099) == -90",
        "acosd(1.00099) == 0",
        "acosd(-1.00099) == 180",
    ] {
        let graph = guarded(source);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(vm.current_state(vm.active_states()[0].0), Some(1), "{source}");
        assert!(vm.unsupported().conditions.is_empty(), "{source}");
    }
}

