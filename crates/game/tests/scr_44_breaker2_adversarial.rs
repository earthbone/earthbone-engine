// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

#[test]
fn scr_44_malformed_scene_is_false_through_papyrus() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), 0x800)
            .zsub(Tag(*b"EDID"), "BrokenScene")
            .sub(Tag(*b"ANAM"), [0]),
    );
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Scr44Malformed", "Scene")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("scene", "Scene")
                .param("action", "Int")
                .local("result", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("IsActionComplete"),
                        ident("scene"),
                        ident("result"),
                        ident("action"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let target = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x800,
    }));
    let thread = game
        .vm
        .call_global("Scr44Malformed", "Run", &[target, Value::Int(42)])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(false)));
    assert!(game.traces().iter().all(|line| !line.contains("error")));
}
