// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-01 regression coverage for cell events under lazy reference binding.

use crate::scr_05_lazy_cell_binding::{cell_id, fixture, REFERENCE};
use soe_esm::{records::Transform, GlobalFormId};
use soe_game::{
    world::{Doorway, Threshold},
    Game, Handle, Pending,
};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::CallStatic,
};

fn reference_id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: REFERENCE,
    }
}

fn game() -> Game {
    let mut game = fixture();
    let mut script = ScriptBuilder::new("Scr05Present", "ObjectReference");
    for name in [
        "OnCellAttach",
        "OnLoad",
        "OnCellLoad",
        "OnCellDetach",
        "OnUnload",
    ] {
        script = script.function(
            FunctionBuilder::new(name, "None")
                .ins(
                    CallStatic,
                    vec![s("Debug"), s("Trace"), ident("::nonevar"), s(name)],
                )
                .build(),
        );
    }
    game.load_script(&script.bytes()).unwrap();
    game.bind();
    game
}

fn cell_events(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|name| name.starts_with("OnCell") || name == "OnLoad" || name == "OnUnload")
        .collect()
}

#[test]
fn cell_attach_world_change_binds_before_delivering_events() {
    let mut game = game();
    game.world_mut().change_cell(Some(cell_id()));
    assert!(game
        .world()
        .pending
        .iter()
        .any(|event| matches!(event, Pending::ReferenceEvent { .. })));
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_game_change_exposes_bound_nonzero_events() {
    let mut game = game();
    game.change_cell(Some(cell_id()));
    let object = game
        .world()
        .object_id(Handle::Form(reference_id()))
        .unwrap();
    assert_eq!(game.vm.scripts_on(object), ["Scr05Present"]);
    let events: Vec<_> = game
        .world()
        .pending
        .iter()
        .filter_map(|event| match event {
            Pending::Event { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(events, ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_prefetch_stays_unbound_until_doorway_commit() {
    let mut game = game();
    let prefetched = game.world_mut().prefetch_cell(cell_id());
    assert!(prefetched > 0);
    assert!(game.world().pending.is_empty());
    assert_eq!(game.world().object_id(Handle::Form(reference_id())), None);
    let way = Doorway {
        door: reference_id(),
        to: Some(cell_id()),
        landing: Transform {
            position: [0.0; 3],
            rotation: [0.0; 3],
        },
        cold: true,
        prefetched,
        threshold: Threshold::new([0.0; 3], [0.0, 1.0, 0.0]),
    };
    game.world_mut().commit_doorway(&way);
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_streaming_neighbor_binds_without_a_focus_cell() {
    let mut game = game();
    game.world_mut()
        .attach_streaming_cells(&[cell_id()])
        .unwrap();
    assert_eq!(game.world().loaded_cell(), None);
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_streaming_delta_binds_every_arrival() {
    let mut game = game();
    game.update_streaming_cells(cell_id(), &[cell_id()], &[])
        .unwrap();
    let object = game
        .world()
        .object_id(Handle::Form(reference_id()))
        .unwrap();
    assert_eq!(game.vm.scripts_on(object), ["Scr05Present"]);
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_disabled_reference_skips_only_load() {
    let mut game = game();
    game.world_mut().reference(reference_id()).unwrap().enabled = false;
    game.world_mut().change_cell(Some(cell_id()));
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnCellLoad"]);
}

#[test]
fn cell_attach_and_detach_before_pump_preserve_the_event_sequence() {
    let mut game = game();
    game.world_mut().change_cell(Some(cell_id()));
    game.world_mut().change_cell(None);
    assert!(game.run_until_idle(8));
    assert_eq!(
        cell_events(&game),
        [
            "OnCellAttach",
            "OnLoad",
            "OnCellLoad",
            "OnCellDetach",
            "OnUnload"
        ]
    );
}

#[test]
fn cell_attach_pending_events_survive_save_load_without_duplicate_scripts() {
    let mut game = game();
    game.world_mut().change_cell(Some(cell_id()));
    let saved = game.save_state().unwrap();
    assert!(game.run_until_idle(8));
    game.load_state(&saved).unwrap();
    assert!(game.run_until_idle(8));
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
    let object = game
        .world()
        .object_id(Handle::Form(reference_id()))
        .unwrap();
    assert_eq!(game.vm.scripts_on(object), ["Scr05Present"]);
    let bound = game.save_state().unwrap();
    game.load_state(&bound).unwrap();
    assert!(game.run_until_idle(8));
    assert_eq!(game.vm.scripts_on(object), ["Scr05Present"]);
    assert_eq!(cell_events(&game), ["OnCellAttach", "OnLoad", "OnCellLoad"]);
}

#[test]
fn cell_attach_saved_events_keep_their_positions_among_calls_and_bindings() {
    let mut game = game();
    let object = game.object_of_form(reference_id());
    game.world_mut().pending = vec![
        Pending::ReferenceEvent {
            reference: reference_id(),
            name: "OnCellAttach".into(),
        },
        Pending::BindReference {
            object,
            base: reference_id(),
        },
        Pending::CallScript {
            object,
            script: "Scr05Present".into(),
            function: "OnLoad".into(),
            args: vec![],
        },
        Pending::ReferenceEvent {
            reference: reference_id(),
            name: "OnCellLoad".into(),
        },
        Pending::Event {
            object,
            name: "OnUnload".into(),
            args: vec![],
        },
        Pending::CallScript {
            object,
            script: "Scr05Present".into(),
            function: "OnLoad".into(),
            args: vec![],
        },
        Pending::BindReference {
            object,
            base: reference_id(),
        },
        Pending::ReferenceEvent {
            reference: reference_id(),
            name: "OnCellDetach".into(),
        },
    ];
    let expected = format!("{:?}", game.world().pending);
    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    assert_eq!(format!("{:?}", game.world().pending), expected);
}
