// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::WeatherRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn malformed_acceleration_flag_does_not_commit_the_valid_override_flag() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 1));
    plugin.push_top(Record::new(Tag::new(b"WTHR"), 2));
    let mut order = LoadOrder::new();
    order.add("Scr54MalformedAcceleration.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut()
        .set_active_weather(id(1), true, false)
        .unwrap();
    let previous = game.world().weather_request();

    let script = ScriptBuilder::new("Scr54MalformedAcceleration", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "Weather")
                .param("override", "Bool")
                .param("accelerate", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("SetActive"),
                        ident("target"),
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
        Value::Object(game.object_of_form(id(2))),
        Value::Bool(false),
        Value::string("true"),
    ];
    let thread = game
        .vm
        .call_global("Scr54MalformedAcceleration", "Run", &args)
        .unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));

    assert_eq!(game.world().weather_request(), previous);
    assert_eq!(
        previous,
        Some(WeatherRequest {
            weather: id(1),
            override_weather: true,
            accelerate: false,
        })
    );
    assert!(game.vm.log.contains("accelerate flag must be a Bool"));
}
