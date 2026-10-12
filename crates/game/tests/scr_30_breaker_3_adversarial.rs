// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU32, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;

static SCRIPT_ID: AtomicU32 = AtomicU32::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn call(game: &mut Game, name: &str, arg_type: &str, arg: Value) {
    let script = format!("Scr30Breaker3{}", SCRIPT_ID.fetch_add(1, Ordering::Relaxed));
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("value", arg_type)
        .ins(
            CallStatic,
            vec![s("Game"), s(name), ident("unused"), ident("value")],
        )
        .build();
    game.load_script(&ScriptBuilder::new(&script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &[arg]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_30_camera_target_switches_actor_and_ignores_non_actor_without_resetting_geometry() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Target Actor",
        "Target Actor",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 0x801));
    let mut order = LoadOrder::new();
    order.add("Scr30Camera.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let actor = game.world_mut().place_at(id(0x800), player, false);
    let invalid = id(0x801);
    let actor_object = Value::Object(game.object_of_form(actor));

    call(&mut game, "SetCameraTarget", "Actor", actor_object);
    assert_eq!(game.world().camera_controls.target, Some(actor));

    call(
        &mut game,
        "ShowFirstPersonGeometry",
        "Bool",
        Value::Bool(false),
    );
    let invalid_object = Value::Object(game.object_of_form(invalid));
    call(&mut game, "SetCameraTarget", "Actor", invalid_object);
    assert_eq!(game.world().camera_controls.target, Some(actor));
    assert!(!game.world().camera_controls.first_person_geometry);

    let player_object = Value::Object(game.object_of_form(player));
    call(&mut game, "SetCameraTarget", "Actor", player_object);
    assert_eq!(game.world().camera_controls.target, Some(player));
    assert!(!game.world().camera_controls.first_person_geometry);
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}
