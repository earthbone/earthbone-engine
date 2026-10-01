// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const FILTER: u32 = 0x801;

#[test]
fn scr_18_empty_authored_filter_is_returned_as_a_valid_form_list() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"GNAM"), FILTER.to_le_bytes()));
    plugin.push_top(Record::new(T(b"FLST"), FILTER));
    let mut order = LoadOrder::new();
    order.add("Scr18Empty.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = "Scr18EmptyProbe";
    let bytes = ScriptBuilder::new(script, "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("base", "ActorBase")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![s("GetGiftFilter"), ident("base"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let base = game.object_of_form(GlobalFormId {
        file: 0,
        object: BASE,
    });
    let result = game
        .vm
        .call_global(script, "Run", &[Value::Object(base)])
        .unwrap();
    game.run_until_idle(64);
    let filter = game.object_of_form(GlobalFormId {
        file: 0,
        object: FILTER,
    });
    assert_eq!(game.vm.thread_result(result), Some(Value::Object(filter)));
    assert!(game
        .world_mut()
        .form_list(GlobalFormId {
            file: 0,
            object: FILTER
        })
        .unwrap()
        .is_empty());
}
