// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

#[test]
fn effect_shader_rejects_non_reference_targets_and_stop_releases_the_matching_request() {
    let t = Tag::new;
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(t(b"EFSH"), 0x801));
    plugin.push_top(Record::new(t(b"ACTI"), 0x802));
    plugin.push_top(Record::new(t(b"REFR"), 0x803).sub(t(b"NAME"), 0x802u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order
        .add("Shaders.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    let call = |function: &str, method: &str| {
        FunctionBuilder::new(function, "Var")
            .global()
            .param("shader", "Form")
            .param("target", "Form")
            .local("result", "Var")
            .ins(
                CallMethod,
                vec![s(method), ident("shader"), ident("result"), ident("target")],
            )
            .ins(Return, vec![ident("result")])
            .build()
    };
    let bytes = ScriptBuilder::new("ShaderProbe", "Form")
        .function(call("Run", "Play"))
        .function(call("RunStop", "Stop"))
        .bytes();
    game.load_script(&bytes).unwrap();
    let shader = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x801,
    }));
    let invalid = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x802,
    }));
    let target = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x803,
    }));
    let thread = game
        .vm
        .call_global("ShaderProbe", "Run", &[shader.clone(), invalid])
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert!(game.world_mut().take_effect_shader_requests().is_empty());
    let thread = game
        .vm
        .call_global("ShaderProbe", "Run", &[shader.clone(), target.clone()])
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    let started = game.world_mut().take_effect_shader_requests();
    assert_eq!(started.len(), 1);
    assert!(started[0].playing);
    let thread = game
        .vm
        .call_global("ShaderProbe", "RunStop", &[shader, target])
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    let stopped = game.world_mut().take_effect_shader_requests();
    assert_eq!(stopped.len(), 1);
    assert!(!stopped[0].playing);
}
