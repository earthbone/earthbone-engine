// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for SCR-23 Debug natives.

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::DebugHostRequest;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x200;
const OTHER_CELL: u32 = 0x201;
const REF: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL).zsub(T(b"EDID"), "DebugCell"));
    plugin.push_top(Record::new(T(b"CELL"), OTHER_CELL).zsub(T(b"EDID"), "OtherCell"));
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
        world.reference(player).unwrap().cell = Some(id(CELL));
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
fn scr_23_foot_ik_can_be_enabled_and_disabled_repeatedly() {
    let mut game = game();
    for (i, enabled) in [true, false, true, false].into_iter().enumerate() {
        run_global(
            &mut game,
            &format!("FootIkProbe{i}"),
            "SetFootIK",
            "enabled",
            "Bool",
            Value::Bool(enabled),
        );
        assert_eq!(game.world().debug_foot_ik(), enabled);
    }
}

#[test]
fn scr_23_show_position_requires_placed_reference_and_deduplicates() {
    let mut game = game();
    let target = game.form("DebugTarget").unwrap();
    let object = game.object_of_form(target);
    for script in ["ShowOnce", "ShowTwice"] {
        run_global(
            &mut game,
            script,
            "ShowRefPosition",
            "reference",
            "ObjectReference",
            Value::Object(object),
        );
    }
    assert_eq!(game.world().debug_ref_positions(), vec![target]);

    let base = game.form("DebugTest.esm:00000400").unwrap_or(id(0x400));
    let base_object = game.object_of_form(base);
    run_global(
        &mut game,
        "ShowBaseObject",
        "ShowRefPosition",
        "reference",
        "ObjectReference",
        Value::Object(base_object),
    );
    assert_eq!(game.world().debug_ref_positions(), vec![target]);
    assert!(game.vm.log.messages().iter().any(|message| {
        message
            .to_string()
            .contains("Debug.ShowRefPosition: invalid reference")
    }));
}

#[test]
fn scr_23_player_move_waits_for_host_completion_and_rejects_missing_name() {
    let mut game = game();
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
    let target = game.form("DebugTarget").unwrap();
    assert!(matches!(
        game.world().debug_host_requests.last(),
        Some(DebugHostRequest::MovePlayerToReference { reference, .. }) if *reference == target
    ));
    let player_id = game.player();
    assert_eq!(
        game.world().reference_ref(player_id).unwrap().position,
        [0.0; 3]
    );

    let Some(DebugHostRequest::MovePlayerToReference { latent, .. }) =
        game.world_mut().debug_host_requests.pop()
    else {
        panic!("expected the queued reference move");
    };
    assert!(game.world_mut().debug_player_move_to_reference(target));
    game.vm.resolve_latent(latent, Value::Float(0.0));
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(0.0)));
    let moved = game.world();
    let player = moved.reference_ref(player_id).unwrap();
    assert_eq!(player.position, [10.0, 20.0, 30.0]);
    assert_eq!(player.rotation, [0.1, 0.2, 0.3]);
    assert_eq!(player.cell, Some(id(CELL)));
    drop(moved);

    let missing = FunctionBuilder::new("Run", "Float")
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
        &ScriptBuilder::new("PlayerMoveMissing", "Form")
            .function(missing)
            .bytes(),
    )
    .unwrap();
    let missing_thread = game
        .vm
        .call_global("PlayerMoveMissing", "Run", &[Value::string("NoSuchRef")])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(
        game.vm.thread_result(missing_thread),
        Some(Value::Float(0.0))
    );
    assert!(game.vm.log.messages().iter().any(|message| {
        message
            .to_string()
            .contains("destination reference not found")
    }));
}

#[test]
fn scr_23_move_to_cell_changes_origin_and_missing_cell_is_noop() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.reference(player).unwrap().position = [5.0, 6.0, 7.0];
    }
    run_global(
        &mut game,
        "MoveCellValid",
        "MoveToCell",
        "cell",
        "String",
        Value::string("OtherCell"),
    );
    {
        let world = game.world();
        let moved = world.reference_ref(player).unwrap();
        assert_eq!(moved.position, [0.0; 3]);
        assert_eq!(moved.cell, Some(id(OTHER_CELL)));
    }
    run_global(
        &mut game,
        "MoveCellMissing",
        "MoveToCell",
        "cell",
        "String",
        Value::string("MissingCell"),
    );
    {
        let world = game.world();
        let moved = world.reference_ref(player).unwrap();
        assert_eq!(moved.position, [0.0; 3]);
        assert_eq!(moved.cell, Some(id(OTHER_CELL)));
    }
    assert!(game.vm.log.messages().iter().any(|message| {
        message
            .to_string()
            .contains("Debug.MoveToCell: destination cell not found")
    }));
}
