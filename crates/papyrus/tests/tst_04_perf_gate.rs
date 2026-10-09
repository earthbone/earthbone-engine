#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};
use soe_papyrus::{Value, Vm};

fn parser_script_bytes() -> Vec<u8> {
    let mut function = FunctionBuilder::new("Count", "Int").local("index", "Int");
    for _ in 0..256 {
        function = function.ins(Opcode::Nop, Vec::new());
    }
    soe_papyrus::asm::write(
        &ScriptBuilder::new("Tst04Parser", "")
            .function(function.ins(Opcode::Return, vec![i(0)]).build())
            .build(),
    )
}

fn vm_script_bytes() -> Vec<u8> {
    let count = FunctionBuilder::new("Count", "Int")
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
    soe_papyrus::asm::write(&ScriptBuilder::new("Tst04Vm", "").function(count).build())
}

#[test]
fn tst_04_pex_parser_reads_the_synthetic_script_body() {
    let parsed = pex::parse(&parser_script_bytes()).expect("synthetic PEX parses");
    assert_eq!(
        parsed.object().function_in("", "Count").unwrap().code.len(),
        257
    );
}

#[test]
fn tst_04_vm_benchmark_loop_returns_the_expected_sum() {
    let mut vm = Vm::new();
    vm.load_bytes(&vm_script_bytes())
        .expect("synthetic PEX loads");
    let thread = vm
        .call_global("Tst04Vm", "Count", &[Value::Int(100)])
        .expect("hot loop starts");
    assert!(vm.run_until_idle(100_000), "VM hot loop reaches idle");
    assert_eq!(vm.thread_result(thread), Some(Value::Int(4_950)));
}
