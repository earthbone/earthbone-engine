// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Value;

#[test]
fn utility_budget_indices_outside_registry_do_not_alias_valid_budgets() {
    let mut game = Game::new(LoadOrder::new());
    let script = soe_papyrus::asm::ScriptBuilder::new("Scr51Breaker", "Form")
        .function(
            soe_papyrus::asm::FunctionBuilder::new("Run", "Var")
                .global()
                .param("index", "Int")
                .local("result", "Var")
                .ins(
                    soe_papyrus::Opcode::CallStatic,
                    vec![
                        soe_papyrus::asm::s("Utility"),
                        soe_papyrus::asm::s("GetCurrentBudget"),
                        soe_papyrus::asm::ident("result"),
                        soe_papyrus::asm::ident("index"),
                    ],
                )
                .ins(soe_papyrus::Opcode::Return, vec![soe_papyrus::asm::ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).expect("script loads");
    for index in [-1, 2, i32::MIN, i32::MAX] {
        let thread = game
            .vm
            .call_global("Scr51Breaker", "Run", &[Value::Int(index)])
            .expect("call starts");
        game.run_until_idle(8);
        assert_eq!(game.vm.thread_result(thread), Some(Value::Int(0)));
    }
}

