// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const WEATHER_A: u32 = 1;
const WEATHER_B: u32 = 2;
const GLOBAL: u32 = 3;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(weather(WEATHER_A, 200));
    plugin.push_top(weather(WEATHER_B, 100));
    plugin.push_top(Record::new(Tag::new(b"GLOB"), GLOBAL));
    plugin.push_top(
        Record::new(Tag::new(b"CLMT"), 20)
            .sub(b"TNAM", [36, 48, 108, 120, 50, 0xc3])
            .sub(
                b"WLST",
                [WEATHER_A, 100, 0]
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(Record::new(Tag::new(b"WRLD"), 21).sub(b"CNAM", 20_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag::new(b"CELL"), 22)
            .sub(b"EDID", b"Exterior\0".to_vec())
            .sub(b"DATA", [0_u8])
            .sub(b"XCCM", 23_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag::new(b"REGN"), 23).sub(b"WNAM", 21_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag::new(b"CELL"), 24)
            .sub(b"EDID", b"OtherExterior\0".to_vec())
            .sub(b"DATA", [0_u8])
            .sub(b"XCCM", 25_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag::new(b"REGN"), 25)
            .sub(b"WNAM", 21_u32.to_le_bytes())
            .sub(b"RDAT", [3_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat())
            .sub(
                b"RDWT",
                [WEATHER_A, 100, 0]
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Weather.esm", plugin).unwrap();
    Game::new(order)
}

fn weather(object: u32, tint: u8) -> Record {
    let colors = (0..17)
        .flat_map(|_| {
            [tint / 2, tint, tint / 3, tint / 10]
                .map(|v| [v, v, v, 0])
                .concat()
        })
        .collect::<Vec<_>>();
    let mut data = vec![0; 19];
    data[3] = 255;
    Record::new(Tag::new(b"WTHR"), object)
        .sub(b"NAM0", colors)
        .sub(b"DATA", data)
        .sub(
            b"FNAM",
            [0.0_f32, 1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        )
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
    assert!(non_weather_error
        .to_string()
        .contains("requires a WTHR form"));
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

#[test]
fn scr_54_set_active_native_flags_drive_the_host_sky_frame() {
    let mut game = game();
    game.world_mut().sky_frame(id(22)).unwrap();
    call(
        &mut game,
        WEATHER_B,
        &[Value::Bool(true), Value::Bool(true)],
    );

    let started = game.world_mut().sky_frame(id(22)).unwrap().unwrap();
    assert_eq!(started.weather, id(WEATHER_B));
    assert_eq!(started.transition, 0.0);

    let overridden = game.world_mut().sky_frame(id(24)).unwrap().unwrap();
    assert_eq!(overridden.weather, id(WEATHER_B));

    game.world_mut().advance_clock(0.2);
    let accelerated = game.world_mut().sky_frame(id(22)).unwrap().unwrap();
    assert_eq!(accelerated.transition, 1.0);
    assert!(accelerated.colors[0][0] < started.colors[0][0]);
}
