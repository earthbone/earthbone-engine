// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-23 checks for the four stateful Debug natives.
//!
//! The CK Debug page documents SetFootIK and PlayerMoveToAndWait signatures
//! and effects. The local page also documents ShowRefPosition as a development
//! tripod request. Cell and reference moves use the runtime's resolved form
//! table, then expose placement changes to the host's existing pose sync.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x200;
const REF: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_debug_destinations() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL).zsub(T(b"EDID"), "DebugCell"));
    plugin.push_top(Record::new(T(b"STAT"), 0x400));
    let mut target = Record::new(T(b"REFR"), REF)
        .sub(T(b"NAME"), 0x400_u32.to_le_bytes())
        .sub(
            T(b"DATA"),
            Transform {
                position: [10.0, 20.0, 30.0],
                rotation: [0.1, 0.2, 0.3],
            }
            .to_bytes(),
        )
        .zsub(T(b"EDID"), "DebugTarget");
    target.placement.cell = Some(CELL);
    plugin.push_top(target);
    let mut order = LoadOrder::new();
    order.add("DebugTest.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).unwrap();
        reference.position = [1.0, 2.0, 3.0];
        reference.cell = Some(id(CELL));
        world.change_cell(Some(id(CELL)));
    }
    game.start();
    game
}

fn run_global(game: &mut Game, script: &str, native: &str, param: &str, ty: &str, value: Value) {
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param(param, ty)
        .ins(
            CallStatic,
            vec![s("Debug"), s(native), ident("::nonevar"), ident(param)],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm.call_global(script, "Run", &[value]).unwrap();
    game.run_until_idle(64);
}

#[test]
fn scr_23_set_foot_ik_changes_world_setting() {
    let mut game = game_with_debug_destinations();
    run_global(
        &mut game,
        "FootIkProbe",
        "SetFootIK",
        "enabled",
        "Bool",
        Value::Bool(false),
    );
    assert!(!game.world().debug_foot_ik());
}

#[test]
fn scr_23_show_ref_position_records_resolved_reference_and_rejects_alias() {
    let mut game = game_with_debug_destinations();
    let target = game.form("DebugTarget").unwrap();
    let object = game.object_of_form(target);
    run_global(
        &mut game,
        "ShowPositionProbe",
        "ShowRefPosition",
        "reference",
        "ObjectReference",
        Value::Object(object),
    );
    assert_eq!(game.world().debug_ref_positions(), vec![target]);
    let alias = game.object_of(soe_game::Handle::Alias {
        quest: target,
        id: 0,
    });
    run_global(
        &mut game,
        "ShowPositionInvalidProbe",
        "ShowRefPosition",
        "reference",
        "ObjectReference",
        Value::Object(alias),
    );
    assert!(game
        .vm
        .log
        .messages()
        .iter()
        .any(|message| message.to_string().contains("invalid reference")));
}

#[test]
fn scr_23_player_move_to_and_wait_moves_player_to_reference() {
    let mut game = game_with_debug_destinations();
    let function = FunctionBuilder::new("Run", "Float")
        .global()
        .param("destination", "String")
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("PlayerMoveToAndWait"),
                ident("result"),
                ident("destination"),
            ],
        )
        .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("PlayerMoveProbe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("PlayerMoveProbe", "Run", &[Value::string("DebugTarget")])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), None);
    assert!(matches!(
        game.world().debug_host_requests.last(),
        Some(soe_game::world::DebugHostRequest::MovePlayerToReference {
            reference,
            ..
        }) if *reference == game.form("DebugTarget").unwrap()
    ));
    let target = game.form("DebugTarget").unwrap();
    assert!(game.world_mut().debug_player_move_to_reference(target));
    let Some(soe_game::world::DebugHostRequest::MovePlayerToReference { latent, .. }) =
        game.world_mut().debug_host_requests.pop()
    else {
        unreachable!();
    };
    game.vm.resolve_latent(latent, Value::Float(0.0));
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(0.0)));
    let player = game.player();
    let world = game.world();
    let moved = world.reference_ref(player).unwrap();
    assert_eq!(moved.position, [10.0, 20.0, 30.0]);
    assert_eq!(moved.rotation, [0.1, 0.2, 0.3]);
    assert_eq!(moved.cell, Some(id(CELL)));
    drop(world);
    let function = FunctionBuilder::new("Run", "Float")
        .global()
        .param("destination", "String")
        .local("result", "Var")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("PlayerMoveToAndWait"),
                ident("result"),
                ident("destination"),
            ],
        )
        .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("PlayerMoveMissingProbe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global(
            "PlayerMoveMissingProbe",
            "Run",
            &[Value::string("MissingReference")],
        )
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(0.0)));
    assert!(game.vm.log.messages().iter().any(|message| {
        message
            .to_string()
            .contains("destination reference not found")
    }));
}

#[test]
fn scr_23_move_to_cell_resolves_editor_id_and_reports_missing_cell() {
    let mut game = game_with_debug_destinations();
    run_global(
        &mut game,
        "MoveToCellProbe",
        "MoveToCell",
        "cell",
        "String",
        Value::string("DebugCell"),
    );
    let player = game.player();
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        [0.0; 3]
    );
    run_global(
        &mut game,
        "MoveToMissingCellProbe",
        "MoveToCell",
        "cell",
        "String",
        Value::string("MissingCell"),
    );
    assert!(game
        .vm
        .log
        .messages()
        .iter()
        .any(|message| message.to_string().contains("destination cell not found")));
}
