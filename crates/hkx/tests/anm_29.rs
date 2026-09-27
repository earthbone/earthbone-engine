// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

struct StateEntry {
    state_id: i32,
    value: i32,
}

fn graph(
    current_state: i32,
    initial_i_state: i32,
    enabled: bool,
    entries: &[StateEntry],
) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, graph);
    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(graph, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);
    b.array_strings(strings, 48, &["Other", "iState"]);
    let infos = b.array_structs(data, 32, 6, 2);
    b.set_i8(infos, 4, 3);
    b.set_i8(infos + 6, 4, 3);
    b.array_u32(data, 64, &[]);
    b.array_i32(values, 16, &[9, initial_i_state]);

    let wrapper = b.object("hkbModifierGenerator", 96);
    b.ptr(graph, 128, wrapper);
    let manager = b.object("BSIStateManagerModifier", 128);
    b.ptr(wrapper, 72, manager);
    b.set_bool(manager, 72, enabled);
    b.set_i32(manager, 80, 1);
    let machine = b.object("hkbStateMachine", 264);
    b.ptr(wrapper, 80, machine);
    b.string(machine, 56, "StateMachine");
    b.set_i32(machine, 104, current_state);
    b.set_i8(machine, 133, 32);
    let state = b.object("hkbStateMachineStateInfo", 120);
    b.string(state, 96, "Current");
    b.set_i32(state, 104, current_state);
    b.set_f32(state, 108, 1.0);
    b.set_bool(state, 112, true);
    b.array_ptrs(machine, 144, &[state]);

    let state_data = b.array_structs(manager, 88, 16, entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let offset = state_data + (index as u32) * 16;
        b.ptr(offset, 0, machine);
        b.set_i32(offset, 8, entry.state_id);
        b.set_i32(offset, 12, entry.value);
    }

    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

#[test]
fn anm_29_matching_entry_uses_declared_i_state_variable_index() {
    let graph = graph(
        2,
        0,
        true,
        &[
            StateEntry {
                state_id: 1,
                value: 11,
            },
            StateEntry {
                state_id: 2,
                value: 22,
            },
        ],
    );
    let manager = graph
        .nodes
        .iter()
        .find(|node| node.class == "BSIStateManagerModifier")
        .unwrap();
    assert!(matches!(manager.kind, NodeKind::IStateManager { .. }));

    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(1.0 / 60.0);

    assert_eq!(vm.variables.word(0), Some(9));
    assert_eq!(vm.variables.word(1), Some(22));
}

#[test]
fn anm_29_unmatched_state_keeps_the_existing_i_state_value() {
    let graph = graph(
        2,
        7,
        true,
        &[StateEntry {
            state_id: 1,
            value: 11,
        }],
    );
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(1.0 / 60.0);

    assert_eq!(vm.variables.word(0), Some(9));
    assert_eq!(vm.variables.word(1), Some(7));
}

#[test]
fn anm_29_disabled_manager_does_not_publish_state() {
    let graph = graph(
        2,
        7,
        false,
        &[StateEntry {
            state_id: 2,
            value: 22,
        }],
    );
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(1.0 / 60.0);

    assert_eq!(vm.variables.word(1), Some(7));
}
