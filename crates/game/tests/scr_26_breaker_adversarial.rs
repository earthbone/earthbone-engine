// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"REFR"), 1));
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 2));
    plugin.push_top(
        Record::new(Tag::new(b"GLOB"), 3)
            .zsub(Tag::new(b"EDID"), "GameHour")
            .sub(Tag::new(b"FLTV"), 6.0f32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag::new(b"GMST"), 4)
            .zsub(Tag::new(b"EDID"), "fCameraShakeDuration")
            .sub(Tag::new(b"DATA"), 1.25f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Camera.esm", plugin).unwrap();
    Game::new(order)
}

fn global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr26Breaker{name}{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s("Game"), s(name), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(
            &parameter,
            if matches!(value, Value::Object(_)) {
                "ObjectReference"
            } else {
                "Var"
            },
        );
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_26_breaker_shake_uses_defaults_and_the_game_setting_for_zero_duration() {
    let mut game = fixture();
    let player = game.world_mut().player();
    global(&mut game, "ShakeCamera", &[]);
    let shakes = &game.world().camera_shakes;
    assert_eq!(shakes.len(), 1);
    assert_eq!(shakes[0].source, player);
    assert_eq!(shakes[0].strength, 0.5);
    assert_eq!(shakes[0].duration, 1.25);
}

#[test]
fn scr_26_breaker_shake_clamps_strength_at_both_bounds() {
    let mut game = fixture();
    let source = Value::Object(game.object_of_form(id(1)));
    global(
        &mut game,
        "ShakeCamera",
        &[source.clone(), Value::Float(-0.25), Value::Float(0.4)],
    );
    global(
        &mut game,
        "ShakeCamera",
        &[source, Value::Float(1.25), Value::Float(0.6)],
    );
    let shakes = &game.world().camera_shakes;
    assert_eq!(shakes.len(), 2);
    assert_eq!(shakes[0].strength, 0.0);
    assert_eq!(shakes[0].duration, 0.4);
    assert_eq!(shakes[1].strength, 1.0);
    assert_eq!(shakes[1].duration, 0.6);
}

#[test]
fn scr_26_breaker_sungazing_accepts_scaled_direct_look_vectors() {
    let mut game = fixture();
    for look in [[1.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.001, 0.0, 0.0]] {
        game.world_mut().camera_look = look;
        assert_eq!(
            global(&mut game, "IsPlayerSungazing", &[]),
            Value::Bool(true),
            "direct look vector {look:?}"
        );
    }
}

#[test]
fn scr_26_breaker_sun_getters_return_the_declared_float_type() {
    let mut game = fixture();
    for name in ["GetSunPositionX", "GetSunPositionY", "GetSunPositionZ"] {
        assert!(
            matches!(global(&mut game, name, &[]), Value::Float(_)),
            "{name}"
        );
    }
}

#[test]
fn scr_26_zero_length_look_vector_is_not_sungazing() {
    let mut game = fixture();
    game.world_mut().camera_look = [0.0, 0.0, 0.0];

    assert_eq!(
        global(&mut game, "IsPlayerSungazing", &[]),
        Value::Bool(false)
    );
}
