// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, Condition, NodeKind};
use soe_hkx::Packfile;

const TROLL_GUARD: &str =
    "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)";

#[test]
fn installed_troll_expression_condition_is_preserved_on_its_authored_start_state() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed troll condition check");
        return;
    };
    let archive = soe_bsa::Archive::open(
        &std::path::Path::new(&data).join("Skyrim - Animations.bsa"),
    )
    .expect("animation archive opens");
    let bytes = archive
        .read("meshes/actors/troll/behaviors/trollbehavior.hkx")
        .expect("troll behavior exists");
    let packfile = Packfile::parse(&bytes).expect("troll behavior parses");
    let graph = BehaviorGraph::from_packfile(&packfile).expect("troll behavior decodes");
    let matches: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::StateMachine(machine) => Some(machine),
            _ => None,
        })
        .flat_map(|machine| {
            machine.states.iter().flat_map(move |state| {
                state.transitions.iter().filter_map(move |transition| {
                    transition.condition.as_ref().and_then(|condition| {
                        matches!(condition, Condition::Expression(text) if text == TROLL_GUARD)
                            .then_some((state.state_id, machine.start_state_id))
                    })
                })
            })
        })
        .collect();
    assert_eq!(matches.len(), 1, "authored expression condition is unique");
    assert_eq!(matches[0].0, matches[0].1, "guard is on the starting state");
}
