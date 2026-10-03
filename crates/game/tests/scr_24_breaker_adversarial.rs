// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 1));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 10));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 11));
    plugin.push_top(Record::new(Tag::new(b"IMAD"), 20));
    let mut order = LoadOrder::new();
    order.add("Scr24Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, receiver: u32, name: &str, args: &[Value]) {
    let script_name = format!("Scr24Breaker{name}{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("shader", "EffectShader");
    let mut operands = vec![s(name), ident("shader"), ident("unused")];
    let mut values = vec![Value::Object(game.object_of_form(id(receiver)))];
    for (index, value) in args.iter().enumerate() {
        let argument = format!("arg{index}");
        function = function.param(&argument, "Var");
        operands.push(ident(&argument));
        values.push(value.clone());
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_24_explicit_none_uses_psc_default_infinite_duration() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(10)));
    call(&mut game, 1, "Play", &[target, Value::None]);

    let active = game.world().effect_shader_playbacks();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].shader, id(1));
    assert_eq!(active[0].target, id(10));
    assert_eq!(active[0].remaining, None);
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].duration, -1.0);
    assert!(requests[0].playing);
}

#[test]
fn scr_24_wrong_shader_receiver_logs_error_without_mutating_playback() {
    let mut game = game();
    game.world_mut()
        .play_effect_shader(id(1), id(10), 8.0)
        .unwrap();
    game.world_mut().take_effect_shader_requests();
    let before = game.world().effect_shader_playbacks();
    let target = Value::Object(game.object_of_form(id(10)));

    call(&mut game, 20, "Stop", &[target]);

    assert_eq!(game.world().effect_shader_playbacks(), before);
    assert!(
        game.vm.log.contains("has no function Stop()"),
        "unexpected Papyrus log: {:?}",
        game.vm.log.messages()
    );
    assert!(game.world_mut().take_effect_shader_requests().is_empty());
}

#[test]
fn scr_24_stop_removes_only_the_requested_shader_target_pair() {
    let mut game = game();
    let first_target = Value::Object(game.object_of_form(id(10)));
    let other_target = Value::Object(game.object_of_form(id(11)));
    call(
        &mut game,
        1,
        "Play",
        &[first_target.clone(), Value::Float(9.0)],
    );
    call(
        &mut game,
        1,
        "Play",
        &[other_target.clone(), Value::Float(9.0)],
    );

    call(&mut game, 1, "Stop", &[first_target]);

    let active = game.world().effect_shader_playbacks();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].target, id(11));
    let requests = game.world_mut().take_effect_shader_requests();
    assert!(requests
        .iter()
        .any(|request| !request.playing && request.target == id(10)));
}
