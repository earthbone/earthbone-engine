// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    Opcode::{CallMethod, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

#[test]
fn scr_66_message_show_reports_no_selection_for_a_notification() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"MESG"), 0x800).zsub(Tag::new(b"EDID"), "Notice"));
    let mut load_order = LoadOrder::new();
    load_order.add("Scr66.esm", plugin).unwrap();
    let mut game = Game::new(load_order);
    let form = game.form("Notice").unwrap();
    let target = Value::Object(game.object_of_form(form));
    let script = ScriptBuilder::new("Scr66MessageProbe", "")
        .function(
            FunctionBuilder::new("Run", "Int")
                .global()
                .param("target", "Message")
                .local("result", "Int")
                .ins(
                    CallMethod,
                    vec![s("Show"), ident("target"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game
        .vm
        .call_global("Scr66MessageProbe", "Run", &[target])
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Int(-1)));
}
