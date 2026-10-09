// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::MenuRequest, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;

#[test]
fn ui_17_repeated_chargen_calls_preserve_host_request_order() {
    let mut game = Game::new(LoadOrder::new());
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .local("result", "None")
        .ins(CallStatic, vec![s("Game"), s("PrecacheCharGen"), ident("result")])
        .ins(CallStatic, vec![s("Game"), s("PrecacheCharGenClear"), ident("result")])
        .ins(CallStatic, vec![s("Game"), s("PrecacheCharGen"), ident("result")])
        .ins(CallStatic, vec![s("Game"), s("PrecacheCharGenClear"), ident("result")])
        .build();
    game.load_script(&ScriptBuilder::new("Ui17RepeatedCache", "Form").function(function).bytes())
        .unwrap();

    let thread = game.vm.call_global("Ui17RepeatedCache", "Run", &[]).unwrap();
    assert!(game.run_until_idle(16));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert_eq!(
        game.world_mut().menu_requests,
        [
            MenuRequest::PrecacheCharGen,
            MenuRequest::PrecacheCharGenClear,
            MenuRequest::PrecacheCharGen,
            MenuRequest::PrecacheCharGenClear,
        ]
    );
}
