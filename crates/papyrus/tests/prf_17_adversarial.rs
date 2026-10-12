// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{NativeReturn, Opcode, Value, Vm};
use std::cell::Cell;
use std::rc::Rc;

fn scripts() -> Vec<u8> {
    ScriptBuilder::new("Prf17Schedule", "")
        .variable("::completed", "Int", i(0))
        .function(
            FunctionBuilder::new("WaitThenMark", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Prf17Host"), s("Wait"), ident("::nonevar")],
                )
                .ins(
                    Opcode::CallStatic,
                    vec![s("Prf17Host"), s("Mark"), ident("::nonevar")],
                )
                .ins(Opcode::Return, vec![i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("MarkNow", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Prf17Host"), s("Mark"), ident("::nonevar")],
                )
                .ins(Opcode::Return, vec![i(2)])
                .build(),
        )
        .bytes()
}

fn vm() -> (Vm, Rc<Cell<usize>>) {
    let mut vm = Vm::new();
    vm.load_bytes(&scripts()).unwrap();
    let marks = Rc::new(Cell::new(0));
    vm.natives.register_global(
        "Prf17Host",
        "Wait",
        Box::new(|_| NativeReturn::Wait(5.0)),
    );
    let native_marks = Rc::clone(&marks);
    vm.natives.register_global(
        "Prf17Host",
        "Mark",
        Box::new(move |_| {
            native_marks.set(native_marks.get() + 1);
            NativeReturn::Value(Value::None)
        }),
    );
    (vm, marks)
}

#[test]
fn prf_17_waiting_stack_does_not_block_an_independent_stack() {
    let (mut vm, marks) = vm();
    let waiting = vm.call_global("Prf17Schedule", "WaitThenMark", &[]).unwrap();
    let runnable = vm.call_global("Prf17Schedule", "MarkNow", &[]).unwrap();

    assert!(vm.run_until_idle(100));

    assert_eq!(vm.thread_state(waiting), Some(ThreadState::Sleeping(5.0)));
    assert_eq!(vm.thread_result(runnable), Some(Value::Int(2)));
    assert_eq!(marks.get(), 1);
}

#[test]
fn prf_17_sleeping_stack_resumes_once_after_save_and_load() {
    let (mut original, marks) = vm();
    let waiting = original
        .call_global("Prf17Schedule", "WaitThenMark", &[])
        .unwrap();
    assert!(original.run_until_idle(100));
    assert_eq!(marks.get(), 0);
    let saved = original.save_state().unwrap();

    let (mut restored, restored_marks) = vm();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.thread_state(waiting), Some(ThreadState::Sleeping(5.0)));
    restored.advance(4.99);
    restored.run(100);
    assert_eq!(restored_marks.get(), 0);
    restored.advance(0.02);
    restored.run(100);
    assert_eq!(restored_marks.get(), 1);
    restored.advance(10.0);
    restored.run(100);
    assert_eq!(restored_marks.get(), 1);
    assert_eq!(restored.thread_result(waiting), Some(Value::Int(1)));
}
