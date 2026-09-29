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
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 2));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 10));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 11));
    plugin.push_top(Record::new(Tag::new(b"IMAD"), 20));
    let mut order = LoadOrder::new();
    order.add("EffectShaders.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, shader: u32, name: &str, args: &[Value]) {
    let script_name = format!("Scr24Call{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("shader", "EffectShader");
    let mut operands = vec![s(name), ident("shader"), ident("unused")];
    for index in 0..args.len() {
        let arg = format!("arg{index}");
        function = function.param(&arg, "Var");
        operands.push(ident(&arg));
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
    let mut values = vec![Value::Object(game.object_of_form(id(shader)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn active(game: &Game) -> Vec<(u32, u32, Option<f64>)> {
    game.world()
        .effect_shader_playbacks()
        .into_iter()
        .map(|playback| {
            (
                playback.shader.object,
                playback.target.object,
                playback.remaining,
            )
        })
        .collect()
}

#[test]
fn scr_24_play_defaults_to_infinite_and_stop_only_removes_its_pair() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(10)));
    call(&mut game, 1, "Play", &[target]);
    assert_eq!(active(&game), vec![(1, 10, None)]);
}

#[test]
fn scr_24_play_duration_replacement_stop_and_expiry_are_observable() {
    let mut game = game();
    let target_a = Value::Object(game.object_of_form(id(10)));
    let target_b = Value::Object(game.object_of_form(id(11)));
    call(&mut game, 1, "Play", &[target_a.clone(), Value::Float(3.0)]);
    call(
        &mut game,
        1,
        "Play",
        &[target_b.clone(), Value::Float(-2.0)],
    );
    call(&mut game, 2, "Play", &[target_a.clone(), Value::Float(5.0)]);
    call(&mut game, 1, "Play", &[target_a.clone(), Value::Float(4.0)]);
    assert_eq!(
        active(&game),
        vec![(1, 10, Some(4.0)), (1, 11, None), (2, 10, Some(5.0))]
    );
    call(&mut game, 2, "Play", &[target_a.clone(), Value::Float(0.0)]);
    assert_eq!(active(&game), vec![(1, 10, Some(4.0)), (1, 11, None)]);
    call(&mut game, 1, "Stop", std::slice::from_ref(&target_a));
    assert_eq!(active(&game), vec![(1, 11, None)]);
    game.tick(5.0);
    assert_eq!(active(&game), vec![(1, 11, None)]);
}

#[test]
fn scr_24_invalid_arguments_log_errors_without_changing_active_playbacks() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(10)));
    call(&mut game, 1, "Play", &[target.clone(), Value::Float(4.0)]);
    let before = active(&game);
    let bad_target = Value::Object(game.object_of_form(id(20)));
    call(&mut game, 1, "Play", &[bad_target, Value::Float(1.0)]);
    call(
        &mut game,
        1,
        "Play",
        &[target.clone(), Value::Float(f32::NAN)],
    );
    call(
        &mut game,
        1,
        "Play",
        &[target.clone(), Value::string("forever")],
    );
    call(&mut game, 1, "Play", &[]);
    call(&mut game, 1, "Stop", &[]);
    assert_eq!(active(&game), before);
    for expected in [
        "target is not a reference",
        "non-finite effect shader duration",
        "duration must be numeric",
        "target must be a placed reference",
    ] {
        assert!(
            game.vm.log.contains(expected),
            "missing error {expected:?}: {:?}",
            game.vm.log.messages()
        );
    }
    assert!(game.world_mut().stop_effect_shader(id(20), id(10)).is_err());
    assert!(game
        .world_mut()
        .play_effect_shader(id(20), id(10), 1.0)
        .is_err());
    assert!(game
        .world_mut()
        .play_effect_shader(id(1), id(10), f32::INFINITY)
        .is_err());
    assert_eq!(active(&game), before);
}

#[test]
fn scr_24_active_playback_round_trips_and_uses_saved_lifetime() {
    let mut original = game();
    original
        .world_mut()
        .play_effect_shader(id(1), id(10), 4.0)
        .unwrap();
    original
        .world_mut()
        .play_effect_shader(id(2), id(11), -1.0)
        .unwrap();
    original.tick(1.5);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert_eq!(active(&restored), active(&original));
    original.tick(2.5);
    restored.tick(2.5);
    assert_eq!(active(&restored), vec![(2, 11, None)]);
    assert_eq!(original.world().save_state(), restored.world().save_state());
}
