// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-14 drives EffectShader.Play and Stop through Papyrus and checks the
//! transient host request they place on the shared World.

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const SHADER: u32 = 0x801;
const TARGET: u32 = 0x802;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"ACTI"), BASE).zsub(T(b"EDID"), "Mag14Base"));
    plugin.push_top(Record::new(T(b"EFSH"), SHADER).zsub(T(b"EDID"), "Mag14Shader"));
    plugin.push_top(
        Record::new(T(b"REFR"), TARGET)
            .zsub(T(b"EDID"), "Mag14Target")
            .sub(T(b"NAME"), BASE.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("Mag14.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn call_shader(game: &mut Game, method: &str, duration: Option<f32>) {
    let mut operands = vec![s(method), ident("shader"), ident("result"), ident("target")];
    if duration.is_some() {
        operands.push(ident("duration"));
    }
    let call = FunctionBuilder::new("Run", "Var")
        .global()
        .param("shader", "Form")
        .param("target", "Form")
        .param("duration", "Float")
        .local("result", "Var")
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Mag14Probe", "Form")
        .function(call)
        .bytes();
    game.load_script(&bytes).unwrap();
    let shader = game.object_of_form(game.form("Mag14Shader").unwrap());
    let target = game.object_of_form(game.form("Mag14Target").unwrap());
    let thread = game
        .vm
        .call_global(
            "Mag14Probe",
            "Run",
            &[
                Value::Object(shader),
                Value::Object(target),
                Value::Float(duration.unwrap_or_default()),
            ],
        )
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn mag_14_play_and_stop_queue_one_targeted_shader_lifetime() {
    let mut game = game();
    call_shader(&mut game, "Play", Some(4.0));
    let started = game.world_mut().take_effect_shader_requests();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].shader.object, SHADER);
    assert_eq!(started[0].target.object, TARGET);
    assert!(started[0].playing);
    assert_eq!(started[0].duration, 4.0);

    call_shader(&mut game, "Play", None);
    let indefinite = game.world_mut().take_effect_shader_requests();
    assert_eq!(indefinite.len(), 1);
    assert_eq!(indefinite[0].duration, -1.0);
    assert!(indefinite[0].playing);

    call_shader(&mut game, "Stop", None);
    let stopped = game.world_mut().take_effect_shader_requests();
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].shader.object, SHADER);
    assert_eq!(stopped[0].target.object, TARGET);
    assert!(!stopped[0].playing);
    assert!(game.world_mut().take_effect_shader_requests().is_empty());
}
