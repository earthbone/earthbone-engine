// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode;
use soe_papyrus::{Value, Vm};

#[test]
fn benchmark_loop_result_survives_repeated_scheduler_budgets() {
    let function = FunctionBuilder::new("Count", "Int")
        .param("limit", "Int")
        .local("index", "Int")
        .local("total", "Int")
        .local("continue", "Bool")
        .ins(Opcode::Assign, vec![ident("index"), i(0)])
        .ins(Opcode::Assign, vec![ident("total"), i(0)])
        .ins(
            Opcode::CmpLt,
            vec![ident("continue"), ident("index"), ident("limit")],
        )
        .ins(Opcode::JmpF, vec![ident("continue"), i(4)])
        .ins(
            Opcode::IAdd,
            vec![ident("total"), ident("total"), ident("index")],
        )
        .ins(Opcode::IAdd, vec![ident("index"), ident("index"), i(1)])
        .ins(Opcode::Jmp, vec![i(-4)])
        .ins(Opcode::Return, vec![ident("total")])
        .build();
    let bytes = soe_papyrus::asm::write(
        &ScriptBuilder::new("Tst04Budgeted", "")
            .function(function)
            .build(),
    );
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).expect("benchmark PEX loads");
    let thread = vm
        .call_global("Tst04Budgeted", "Count", &[Value::Int(100)])
        .expect("benchmark loop starts");

    assert!(
        !vm.run_until_idle(0),
        "zero budget does not execute the loop"
    );
    for _ in 0..1000 {
        if vm.run_until_idle(7) {
            break;
        }
    }
    assert!(vm.idle(), "repeated budgets eventually complete the loop");
    assert_eq!(vm.thread_result(thread), Some(Value::Int(4_950)));
    assert!(vm.run_until_idle(7), "a completed VM remains idle");
    assert_eq!(vm.thread_result(thread), Some(Value::Int(4_950)));
}
