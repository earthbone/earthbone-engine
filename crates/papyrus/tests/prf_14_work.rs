// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Executed work counts are distinct from scheduler budget consumption.

use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{NativeReturn, Opcode, Value, Vm};
use soe_trace::{Operation, WorkCounts, WorkFrame};

#[test]
fn prf_14_vm_counts_native_and_builtin_calls_only_when_executed() {
    let mut vm = Vm::new();
    vm.natives.register_global(
        "Probe",
        "Call",
        Box::new(|_| NativeReturn::Value(Value::None)),
    );
    vm.load_bytes(
        &ScriptBuilder::new("Counts", "")
            .function(
                FunctionBuilder::new("Run", "None")
                    .global()
                    .ins(Opcode::Nop, vec![])
                    .ins(
                        Opcode::CallStatic,
                        vec![s("Probe"), s("Call"), ident("::nonevar")],
                    )
                    .ins(
                        Opcode::CallStatic,
                        vec![s("Utility"), s("Wait"), ident("::nonevar"), f(1.0)],
                    )
                    .ins(Opcode::Return, vec![none()])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    vm.call_global("Counts", "Run", &[]).unwrap();
    {
        let frame = WorkFrame::begin();
        assert_eq!(vm.run(2), 2);
        assert_eq!(frame.counts(), WorkCounts([0, 0, 0, 2, 1, 0, 0, 0, 0, 0]));
    }
    {
        let frame = WorkFrame::begin();
        assert_eq!(vm.run(100), 1);
        assert_eq!(frame.counts(), WorkCounts([0, 0, 0, 1, 1, 0, 0, 0, 0, 0]));
        assert_eq!(vm.run(100), 0);
        assert_eq!(frame.counts().get(Operation::PapyrusInstructions), 1);
    }
    vm.advance(1.0);
    let frame = WorkFrame::begin();
    assert_eq!(vm.run(100), 1);
    assert_eq!(frame.counts(), WorkCounts([0, 0, 0, 1, 0, 0, 0, 0, 0, 0]));
}

#[test]
fn prf_14_implicit_return_is_not_an_executed_opcode() {
    let mut vm = Vm::new();
    vm.load_bytes(
        &ScriptBuilder::new("Empty", "")
            .function(FunctionBuilder::new("Run", "None").global().build())
            .bytes(),
    )
    .unwrap();
    vm.call_global("Empty", "Run", &[]).unwrap();
    let frame = WorkFrame::begin();
    assert_eq!(vm.run(100), 1, "the scheduler charges an implicit return");
    assert_eq!(frame.counts(), WorkCounts::default());
}
