#![allow(clippy::expect_used)]

use soe_papyrus::asm::{i, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};
use soe_papyrus::{Value, Vm};

#[test]
fn tst_04_parser_accepts_a_minimal_return_only_function() {
    let bytes = soe_papyrus::asm::write(
        &ScriptBuilder::new("Tst04Minimal", "")
            .function(
                FunctionBuilder::new("Count", "Int")
                    .ins(Opcode::Return, vec![i(0)])
                    .build(),
            )
            .build(),
    );

    let parsed = pex::parse(&bytes).expect("minimal PEX parses");
    assert_eq!(
        parsed
            .object()
            .function_in("", "Count")
            .expect("Count function exists")
            .code
            .len(),
        1
    );
}

#[test]
fn tst_04_vm_zero_iteration_loop_returns_zero() {
    let function = FunctionBuilder::new("Count", "Int")
        .param("limit", "Int")
        .local("continue", "Bool")
        .ins(Opcode::CmpLt, vec![ident("continue"), i(0), ident("limit")])
        .ins(Opcode::JmpF, vec![ident("continue"), i(2)])
        .ins(Opcode::Return, vec![i(1)])
        .ins(Opcode::Return, vec![i(0)])
        .build();
    let bytes = soe_papyrus::asm::write(
        &ScriptBuilder::new("Tst04Zero", "")
            .function(function)
            .build(),
    );
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).expect("synthetic PEX loads");
    let thread = vm
        .call_global("Tst04Zero", "Count", &[Value::Int(0)])
        .expect("zero iteration function starts");

    assert!(vm.run_until_idle(100));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(0)));
}

use soe_papyrus::asm::ident;
