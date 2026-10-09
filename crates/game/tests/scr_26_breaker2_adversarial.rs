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
    plugin.push_top(
        Record::new(Tag::new(b"GMST"), 4)
            .zsub(Tag::new(b"EDID"), "fCameraShakeDuration")
            .sub(Tag::new(b"DATA"), 1.25f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Camera.esm", plugin).unwrap();
    Game::new(order)
}

fn global(game: &mut Game, args: &[Value]) {
    let script = format!("Scr26Breaker2{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "Var").global();
    let mut operands = vec![s("Game"), s("ShakeCamera")];
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
        .function(function.ins(CallStatic, operands).ins(Return, vec![]).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap();
}

#[test]
fn scr_26_null_source_uses_player_location_and_default_strength() {
    let mut game = fixture();
    let player = game.world_mut().player();
    global(
        &mut game,
        &[Value::None, Value::Float(0.5), Value::Float(0.0)],
    );

    let shakes = &game.world().camera_shakes;
    assert_eq!(shakes.len(), 1);
    assert_eq!(shakes[0].source, player);
    assert_eq!(shakes[0].strength, 0.5);
    assert_eq!(shakes[0].duration, 1.25);
}

#[test]
fn scr_26_explicit_duration_overrides_the_game_setting() {
    let mut game = fixture();
    let source = Value::Object(game.object_of_form(id(1)));
    global(
        &mut game,
        &[source, Value::Float(0.75), Value::Float(0.4)],
    );

    let shakes = &game.world().camera_shakes;
    assert_eq!(shakes.len(), 1);
    assert_eq!(shakes[0].source, id(1));
    assert_eq!(shakes[0].strength, 0.75);
    assert_eq!(shakes[0].duration, 0.4);
}
