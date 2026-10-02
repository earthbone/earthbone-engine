// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game, npc,
    world::{GameUiRequest, MenuRequest},
};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

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
fn scr_30_race_menu_keeps_both_host_request_paths_without_duplicate_natives() {
    let mut game = Game::new(LoadOrder::new());
    assert!(
        game.vm.natives.duplicates.iter().all(|(class, name, _)| {
            class != "Game"
                || !matches!(
                    name.as_str(),
                    "PrecacheCharGen"
                        | "PrecacheCharGenClear"
                        | "ShowRaceMenu"
                        | "ShowTrainingMenu"
                )
        }),
        "{:?}",
        game.vm.natives.duplicates
    );
    assert_eq!(call_game(&mut game, "ShowRaceMenu", None), Value::None);
    assert_eq!(game.world().menu_requests, [MenuRequest::Race]);
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        [GameUiRequest::ShowRaceMenu]
    );
}

#[test]
fn scr_30_valid_training_menu_keeps_presenter_and_typed_requests() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut class_data = vec![0; 36];
    class_data[5] = 1;
    plugin.push_top(Record::new(Tag::new(b"CLAS"), 0x801).sub(Tag::new(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x802,
            "Trainer",
            "Trainer",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(Tag::new(b"CNAM"), 0x801u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Training.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let trainer = game.world_mut().place_at(
        GlobalFormId {
            file: 0,
            object: 0x802,
        },
        player,
        false,
    );
    let actor = Value::Object(game.object_of_form(trainer));
    assert_eq!(
        call_game(&mut game, "ShowTrainingMenu", Some(("Actor", actor))),
        Value::None
    );
    assert_eq!(game.world().menu_requests, [MenuRequest::Training(trainer)]);
    assert_eq!(game.world_mut().take_training_menu_request(), Some(trainer));
    assert_eq!(game.world_mut().take_training_menu_request(), None);
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
