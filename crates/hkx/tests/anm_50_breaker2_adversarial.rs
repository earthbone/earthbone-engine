// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, Condition, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

const TROLL_GUARD: &str =
    "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)";
const TROLL_BEHAVIOR: &str = "meshes/actors/troll/behaviors/trollbehavior.hkx";

#[test]
fn anm_50_breaker2_installed_guard_trace_covers_both_bounds_and_direction_alternatives() {
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA is required");
    let archive =
        soe_bsa::Archive::open(&std::path::Path::new(&data).join("Skyrim - Animations.bsa"))
            .expect("animation archive opens");
    let bytes = archive.read(TROLL_BEHAVIOR).expect("troll behavior exists");
    let packfile = Packfile::parse(&bytes).expect("troll behavior parses");
    let graph = BehaviorGraph::from_packfile(&packfile).expect("troll behavior decodes");

    let mut matches = Vec::new();
    for node in &graph.nodes {
        let NodeKind::StateMachine(machine) = &node.kind else {
            continue;
        };
        for state in &machine.states {
            for transition in &state.transitions {
                if transition.condition.as_ref().is_some_and(
                    |condition| matches!(condition, Condition::Expression(text) if text == TROLL_GUARD),
                ) {
                    matches.push((node.id, state.state_id, machine.start_state_id, transition.event_id, transition.to_state_id));
                }
            }
        }
    }
    assert_eq!(matches.len(), 1, "authored troll guard should be unique");
    let (machine, source_state, start_state, event, target_state) = matches[0];
    assert_eq!(source_state, start_state, "authored guard starts active");

    for (direction, magnitude, should_transition) in [
        (0.25, 0.3, false),
        (0.25, 0.3001, true),
        (0.25, 0.6999, true),
        (0.25, 0.7, false),
        (0.2499, 0.3, true),
        (0.7501, 0.7, true),
    ] {
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        assert!(vm.variables.set_by_name("staggerDirection", direction));
        assert!(vm.variables.set_by_name("staggerMagnitude", magnitude));
        vm.send_event(event);
        vm.update(0.0);
        let expected = if should_transition {
            target_state
        } else {
            start_state
        };
        assert_eq!(
            vm.current_state(machine),
            Some(expected),
            "direction={direction}, magnitude={magnitude}"
        );
        assert!(vm.unsupported().conditions.is_empty());
    }
}
