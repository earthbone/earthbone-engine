// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

#[test]
fn scr_50_memory_query_reports_unavailable_when_tracking_is_absent() {
    let mut game = Game::new(LoadOrder::new());
    let script = ScriptBuilder::new("Scr50MemoryBreaker", "Utility")
        .function(
            FunctionBuilder::new("Run", "Int")
                .global()
                .local("result", "Int")
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("GetCurrentMemory"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game
        .vm
        .call_global("Scr50MemoryBreaker", "Run", &[])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert!(game
        .traces()
        .iter()
        .any(|line| line.contains("memory tracking is unavailable")));
}
