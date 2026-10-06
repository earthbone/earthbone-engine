// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::WeatherRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

#[test]
fn malformed_optional_flag_does_not_silently_replace_the_weather_request() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 1));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 2));
    let mut order = LoadOrder::new();
    order.add("Scr54WeatherArgs.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut()
        .set_active_weather(GlobalFormId { file: 0, object: 1 }, true, true)
        .unwrap();
    let previous = game.world().weather_request();
    let script = ScriptBuilder::new("Scr54MalformedWeatherFlag", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("weather", "Weather")
                .param("override", "Var")
                .param("accelerate", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("SetActive"),
                        ident("weather"),
                        ident("unused"),
                        ident("override"),
                        ident("accelerate"),
                    ],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let args = [
        Value::Object(game.object_of_form(GlobalFormId { file: 0, object: 2 })),
        Value::string("true"),
        Value::Bool(false),
    ];
    let thread = game
        .vm
        .call_global("Scr54MalformedWeatherFlag", "Run", &args)
        .unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));

    assert_eq!(game.world().weather_request(), previous);
    assert!(game.vm.log.contains("Weather.SetActive"));
    assert_eq!(
        previous,
        Some(WeatherRequest {
            weather: GlobalFormId { file: 0, object: 1 },
            override_weather: true,
            accelerate: true,
        })
    );
}
