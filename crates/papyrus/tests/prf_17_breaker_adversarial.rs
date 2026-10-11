// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{NativeReturn, Opcode, Value, Vm};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn prf_17_latent_wait_leaves_other_stack_runnable() {
    let script = ScriptBuilder::new("Prf17Independent", "")
        .function(
            FunctionBuilder::new("Wait", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Prf17Source"), s("Sleep"), ident("::nonevar")],
                )
                .ins(Opcode::Return, vec![i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("Complete", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Prf17Source"), s("Mark"), ident("::nonevar")],
                )
                .ins(Opcode::Return, vec![i(2)])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&script).unwrap();
    let calls = Rc::new(Cell::new(0));
    vm.natives.register_global(
        "Prf17Source",
        "Sleep",
        Box::new(|_| NativeReturn::Wait(30.0)),
    );
    let observed_calls = Rc::clone(&calls);
    vm.natives.register_global(
        "Prf17Source",
        "Mark",
        Box::new(move |_| {
            observed_calls.set(observed_calls.get() + 1);
            NativeReturn::Value(Value::None)
        }),
    );

    let sleeping = vm.call_global("Prf17Independent", "Wait", &[]).unwrap();
    let runnable = vm.call_global("Prf17Independent", "Complete", &[]).unwrap();
    assert!(vm.run_until_idle(100));

    assert_eq!(vm.thread_state(sleeping), Some(ThreadState::Sleeping(30.0)));
    assert_eq!(vm.thread_result(runnable), Some(Value::Int(2)));
    assert_eq!(calls.get(), 1);
}
