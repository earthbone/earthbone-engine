// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};
use soe_papyrus::{Value, Vm};

#[test]
fn vm_benchmark_workload_resumes_after_each_instruction_budget() {
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
        &ScriptBuilder::new("BudgetedTst04", "")
            .function(function)
            .build(),
    );
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).expect("synthetic PEX loads");
    let thread = vm
        .call_global("BudgetedTst04", "Count", &[Value::Int(4)])
        .expect("hot loop starts");

    let mut calls = 0;
    while !vm.run_until_idle(1) {
        calls += 1;
        assert!(calls < 100, "one-instruction budgets eventually finish");
    }

    assert_eq!(vm.thread_result(thread), Some(Value::Int(6)));
}

#[test]
fn parser_rejects_a_truncated_pex_instead_of_returning_a_partial_script() {
    let script = ScriptBuilder::new("TruncatedTst04", "").build();
    let mut bytes = soe_papyrus::asm::write(&script);
    bytes.truncate(bytes.len() - 1);

    assert!(pex::parse(&bytes).is_err());
}
