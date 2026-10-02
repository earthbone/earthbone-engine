// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};
use soe_papyrus::{Value, Vm};

#[test]
fn parser_workload_preserves_empty_and_single_instruction_functions() {
    for (name, instructions) in [("Empty", 0usize), ("One", 1usize)] {
        let mut function = FunctionBuilder::new(name, "Int");
        if instructions == 1 {
            function = function.ins(Opcode::Nop, Vec::new());
        }
        let script = ScriptBuilder::new("Boundary", "")
            .function(function.ins(Opcode::Return, vec![i(0)]).build())
            .build();
        let parsed = pex::parse(&soe_papyrus::asm::write(&script)).unwrap();
        assert_eq!(
            parsed.object().function_in("", name).unwrap().code.len(),
            instructions + 1
        );
    }
}

#[test]
fn vm_workload_handles_zero_and_one_loop_iterations() {
    let function = FunctionBuilder::new("Count", "Int")
        .param("limit", "Int")
        .local("index", "Int")
        .local("total", "Int")
        .local("keepGoing", "Bool")
        .ins(Opcode::Assign, vec![ident("index"), i(0)])
        .ins(Opcode::Assign, vec![ident("total"), i(0)])
        .ins(
            Opcode::CmpLt,
            vec![ident("keepGoing"), ident("index"), ident("limit")],
        )
        .ins(Opcode::JmpF, vec![ident("keepGoing"), i(4)])
        .ins(
            Opcode::IAdd,
            vec![ident("total"), ident("total"), ident("index")],
        )
        .ins(Opcode::IAdd, vec![ident("index"), ident("index"), i(1)])
        .ins(Opcode::Jmp, vec![i(-4)])
        .ins(Opcode::Return, vec![ident("total")])
        .build();
    let bytes =
        soe_papyrus::asm::write(&ScriptBuilder::new("EdgeVm", "").function(function).build());
    for (limit, expected) in [(0, 0), (1, 0), (2, 1)] {
        let mut vm = Vm::new();
        vm.load_bytes(&bytes).unwrap();
        let thread = vm
            .call_global("EdgeVm", "Count", &[Value::Int(limit)])
            .unwrap();
        assert!(vm.run_until_idle(100));
        assert_eq!(vm.thread_result(thread), Some(Value::Int(expected)));
    }
}
