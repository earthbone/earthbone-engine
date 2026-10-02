// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const WEATHER_A: u32 = 1;
const WEATHER_B: u32 = 2;
const GLOBAL: u32 = 3;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), WEATHER_A));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), WEATHER_B));
    plugin.push_top(Record::new(Tag::new(b"GLOB"), GLOBAL));
    let mut order = LoadOrder::new();
    order.add("Weather.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, weather: u32, args: &[Value]) {
    let script_name = format!("Scr54Call{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "Weather");
    let mut operands = vec![s("SetActive"), ident("target"), ident("unused")];
    for (index, _) in args.iter().enumerate() {
        let name = format!("arg{index}");
        function = function.param(&name, "Bool");
        operands.push(ident(&name));
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(weather)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_54_set_active_defaults_both_optional_arguments_to_false() {
    let mut game = game();
    call(&mut game, WEATHER_A, &[]);
    assert_eq!(
        game.world().weather_request(),
        Some(soe_game::world::WeatherRequest {
            weather: id(WEATHER_A),
            override_weather: false,
            accelerate: false,
        })
    );
}

#[test]
fn scr_54_set_active_preserves_override_and_acceleration_arguments() {
    let mut game = game();
    call(
        &mut game,
        WEATHER_B,
        &[Value::Bool(true), Value::Bool(false)],
    );
    assert_eq!(
        game.world().weather_request(),
        Some(soe_game::world::WeatherRequest {
            weather: id(WEATHER_B),
            override_weather: true,
            accelerate: false,
        })
    );
    call(
        &mut game,
        WEATHER_A,
        &[Value::Bool(false), Value::Bool(true)],
    );
    assert_eq!(
        game.world().weather_request(),
        Some(soe_game::world::WeatherRequest {
            weather: id(WEATHER_A),
            override_weather: false,
            accelerate: true,
        })
    );
}

#[test]
fn scr_54_set_active_world_api_rejects_invalid_forms_without_mutating_request() {
    let game = game();
    game.world_mut()
        .set_active_weather(id(WEATHER_A), true, false)
        .unwrap();
    let previous = game.world().weather_request();
    let non_weather_error = game
        .world_mut()
        .set_active_weather(id(GLOBAL), false, true)
        .unwrap_err();
    assert!(
        non_weather_error
            .to_string()
            .contains("requires a WTHR form")
    );
    assert_eq!(game.world().weather_request(), previous);
    let missing_error = game
        .world_mut()
        .set_active_weather(id(0xFFFF), false, true)
        .unwrap_err();
    assert_eq!(
        missing_error.to_string(),
        format!(
            "saved state: Weather.SetActive requires a WTHR form, got {}",
            id(0xFFFF)
        )
    );
    assert_eq!(game.world().weather_request(), previous);
}
