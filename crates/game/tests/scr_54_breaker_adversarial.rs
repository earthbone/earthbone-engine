// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::WeatherRequest;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const WEATHER: u32 = 1;
const GLOBAL: u32 = 2;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), WEATHER));
    plugin.push_top(Record::new(Tag::new(b"GLOB"), GLOBAL));
    let mut order = LoadOrder::new();
    order.add("Scr54Breaker.esp", plugin).expect("plugin loads");
    Game::new(order)
}

fn call_weather(game: &mut Game, receiver: u32, script_name: &str) {
    let script = ScriptBuilder::new(script_name, "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "Weather")
                .ins(
                    CallMethod,
                    vec![s("SetActive"), ident("target"), ident("unused")],
                )
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).expect("script loads");
    let receiver = Value::Object(game.object_of_form(id(receiver)));
    let thread = game
        .vm
        .call_global(
            script_name,
            "Run",
            &[receiver],
        )
        .expect("thread starts");
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn deleted_winning_weather_is_rejected_without_replacing_the_active_request() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), WEATHER));
    plugin.push_top(
        Record::new(Tag::new(b"WTHR"), 3).with_flags(soe_esm::plugin::flags::DELETED),
    );
    let mut order = LoadOrder::new();
    order.add("DeletedWeather.esp", plugin).expect("plugin loads");
    let mut game = Game::new(order);
    game.world_mut()
        .set_active_weather(id(WEATHER), true, false)
        .expect("live weather accepted");
    let before = game.world().weather_request();

    let result = game.world_mut().set_active_weather(id(3), false, true);

    assert!(result.is_err(), "deleted WTHR is not a selectable form");
    assert_eq!(game.world().weather_request(), before);
}

#[test]
fn papyrus_wrong_receiver_logs_error_and_preserves_the_previous_weather() {
    let mut game = game();
    game.world_mut()
        .set_active_weather(id(WEATHER), true, false)
        .expect("valid weather accepted");
    let before = game.world().weather_request();

    call_weather(&mut game, GLOBAL, "Scr54WrongWeatherReceiver");

    assert_eq!(game.world().weather_request(), before);
    assert!(game
        .vm
        .log
        .contains("has no function SetActive()"),
        "Papyrus log: {:?}",
        game.vm.log.messages()
    );
}

#[test]
fn set_active_omitted_flags_use_vanilla_defaults_through_the_papyrus_binding() {
    let mut game = game();
    let script = ScriptBuilder::new("Scr54WeatherDefaults", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "Weather")
                .ins(
                    CallMethod,
                    vec![s("SetActive"), ident("target"), ident("unused")],
                )
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).expect("script loads");
    let receiver = Value::Object(game.object_of_form(id(WEATHER)));
    let thread = game
        .vm
        .call_global(
            "Scr54WeatherDefaults",
            "Run",
            &[receiver],
        )
        .expect("thread starts");
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert_eq!(
        game.world().weather_request(),
        Some(WeatherRequest {
            weather: id(WEATHER),
            override_weather: false,
            accelerate: false,
        })
    );
}
