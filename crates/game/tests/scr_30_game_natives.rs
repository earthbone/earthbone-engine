// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::MenuRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn call_game(game: &mut Game, name: &str, arg: Option<(&str, Value)>) -> Value {
    let script = format!("Scr30{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Game"), s(name), ident("result")];
    let args = if let Some((arg_type, _)) = &arg {
        function = function.param("value", arg_type);
        operands.push(ident("value"));
        vec![arg.unwrap().1]
    } else {
        Vec::new()
    };
    let function = function
        .ins(CallStatic, operands)
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(&ScriptBuilder::new(&script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(&script, "Run", &args).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_30_precache_chargen_requests_keep_order_and_return_none() {
    let mut game = Game::new(LoadOrder::new());
    assert_eq!(call_game(&mut game, "PrecacheCharGen", None), Value::None);
    assert_eq!(
        call_game(&mut game, "PrecacheCharGenClear", None),
        Value::None
    );
    assert_eq!(
        game.world().menu_requests,
        [
            MenuRequest::PrecacheCharGen,
            MenuRequest::PrecacheCharGenClear
        ]
    );
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}

#[test]
fn scr_30_show_training_menu_carries_actor_and_rejects_missing_actor() {
    let mut game = Game::new(LoadOrder::new());
    let trainer = game.world_mut().player();
    let trainer_arg = Value::Object(game.object_of_form(trainer));
    assert_eq!(
        call_game(&mut game, "ShowTrainingMenu", Some(("Actor", trainer_arg))),
        Value::None
    );
    assert_eq!(game.world().menu_requests, [MenuRequest::Training(trainer)]);
    assert_eq!(
        call_game(&mut game, "ShowTrainingMenu", Some(("Actor", Value::None))),
        Value::None
    );
    assert_eq!(game.world().menu_requests, [MenuRequest::Training(trainer)]);
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}

#[test]
fn scr_30_camera_natives_apply_actor_and_boolean_arguments() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.world_mut().player();
    let actor = Value::Object(game.object_of_form(player));
    assert_eq!(
        call_game(&mut game, "SetCameraTarget", Some(("Actor", actor))),
        Value::None
    );
    assert_eq!(
        call_game(&mut game, "SetCameraTarget", Some(("Actor", Value::None))),
        Value::None
    );
    assert_eq!(
        call_game(
            &mut game,
            "ShowFirstPersonGeometry",
            Some(("Bool", Value::Bool(false)))
        ),
        Value::None
    );
    assert_eq!(game.world().camera_controls.target, Some(player));
    assert!(!game.world().camera_controls.first_person_geometry);
    assert_eq!(
        call_game(
            &mut game,
            "ShowFirstPersonGeometry",
            Some(("Bool", Value::Bool(true)))
        ),
        Value::None
    );
    assert!(game.world().camera_controls.first_person_geometry);
    assert!(game.traces().iter().all(|trace| !trace.contains("error")));
}
