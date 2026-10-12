// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicU32, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, world::MenuRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;

static SCRIPT_ID: AtomicU32 = AtomicU32::new(0);

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn call(game: &mut Game, name: &str, args: &[Value]) {
    let script = format!("Scr30Breaker2{}", SCRIPT_ID.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "None").global();
    let mut operands = vec![s("Game"), s(name), ident("unused")];
    for (index, _) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, "Var");
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(function.ins(CallStatic, operands).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_30_invalid_training_targets_do_not_replace_a_valid_request() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "Trainer",
        "Trainer",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(T(b"ACTI"), 0x801));
    let mut order = LoadOrder::new();
    order.add("Scr30Training.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let trainer = game.world_mut().place_at(id(0x800), player, false);
    let trainer_object = Value::Object(game.object_of_form(trainer));

    call(&mut game, "ShowTrainingMenu", &[trainer_object]);
    for invalid in [
        Value::Object(game.object_of_form(id(0x801))),
        Value::None,
        Value::Int(0),
    ] {
        call(&mut game, "ShowTrainingMenu", &[invalid]);
    }

    assert_eq!(game.world().menu_requests, [MenuRequest::Training(trainer)]);
    assert_eq!(game.world_mut().take_training_menu_request(), Some(trainer));
    assert_eq!(game.world_mut().take_training_menu_request(), None);
}

#[test]
fn scr_30_cache_and_camera_requests_do_not_survive_game_save_load() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.player();
    let player_object = Value::Object(game.object_of_form(player));
    call(&mut game, "PrecacheCharGen", &[]);
    call(&mut game, "PrecacheCharGenClear", &[]);
    call(&mut game, "ShowTrainingMenu", &[player_object.clone()]);
    call(&mut game, "SetCameraTarget", &[player_object]);
    call(&mut game, "ShowFirstPersonGeometry", &[Value::Bool(false)]);

    let saved = game.save_state().unwrap();
    let mut restored = Game::new(LoadOrder::new());
    restored.load_state(&saved).unwrap();

    assert!(restored.world().menu_requests.is_empty());
    assert_eq!(restored.world().camera_controls, Default::default());
    assert_eq!(restored.world_mut().take_training_menu_request(), None);
}
