// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

#[test]
fn no_argument_enables_head_tracking_by_ck_default() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr12Args.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let bytes = ScriptBuilder::new("Scr12HeadProbe", "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("actor", "Form")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![s("SetHeadTracking"), ident("actor"), ident("result")],
                )
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let actor = GlobalFormId {
        file: 0,
        object: 0x801,
    };
    game.world_mut().set_head_tracking(actor, false).unwrap();
    let actor_object = Value::Object(game.object_of_form(actor));
    let thread = game
        .vm
        .call_global("Scr12HeadProbe", "Run", &[actor_object])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(game.world_mut().head_tracking(actor), Some(true));
}
