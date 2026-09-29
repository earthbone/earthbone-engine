// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{Opcode, Value, Vm};

fn waiting_script() -> Vec<u8> {
    ScriptBuilder::new("Sav03Breaker", "")
        .variable("::fired", "Int", i(0))
        .function(
            FunctionBuilder::new("Run", "Int")
                .local("result", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Sav03Breaker"), s("Nested"), ident("result")],
                )
                .ins(Opcode::IAdd, vec![ident("::fired"), ident("::fired"), i(1)])
                .ins(Opcode::Return, vec![ident("::fired")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Nested", "Int")
                .local("marker", "Int")
                .ins(
                    Opcode::IAdd,
                    vec![ident("marker"), i(40), i(2)],
                )
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s("Wait"), ident("::nonevar"), f(5.0)],
                )
                .ins(Opcode::IAdd, vec![ident("marker"), ident("marker"), i(2)])
                .ins(Opcode::Return, vec![ident("marker")])
                .build(),
        )
        .bytes()
}

#[test]
fn sav_03_wait_remaining_time_excludes_menu_time_after_repeated_load() {
    let script = waiting_script();
    let mut original = Vm::new();
    original.load_bytes(&script).unwrap();
    let object = original.create_object("wait owner");
    original.attach(object, "Sav03Breaker").unwrap();
    let thread = original.call(object, "Run", &[]).unwrap();
    original.run(100);
    original.advance(2.0);

    let first_save = original.save_state().unwrap();
    let mut restored = Vm::new();
    restored.load_bytes(&script).unwrap();
    restored.load_state(&first_save).unwrap();
    let second_save = restored.save_state().unwrap();
    let mut restored_again = Vm::new();
    restored_again.load_bytes(&script).unwrap();
    restored_again.load_state(&second_save).unwrap();

    restored_again.advance_clocks(30.0, 0.0, true);
    restored_again.run(100);
    assert_eq!(restored_again.thread_state(thread), Some(ThreadState::Sleeping(5.0)));
    assert_eq!(restored_again.get_var(object, "Sav03Breaker", "::fired"), Some(Value::Int(0)));

    restored_again.advance_clocks(2.99, 0.0, false);
    restored_again.run(100);
    assert_eq!(restored_again.get_var(object, "Sav03Breaker", "::fired"), Some(Value::Int(0)));
    restored_again.advance_clocks(0.02, 0.0, false);
    restored_again.run(100);
    assert_eq!(restored_again.get_var(object, "Sav03Breaker", "::fired"), Some(Value::Int(1)));
    assert_eq!(restored_again.thread_result(thread), Some(Value::Int(1)));
}

#[test]
fn sav_03_nested_wait_resumes_saved_call_frame_once() {
    let script = waiting_script();
    let mut original = Vm::new();
    original.load_bytes(&script).unwrap();
    let object = original.create_object("nested wait owner");
    original.attach(object, "Sav03Breaker").unwrap();
    let thread = original.call(object, "Run", &[]).unwrap();
    original.run(100);
    assert!(matches!(original.thread_state(thread), Some(ThreadState::Sleeping(_))));
    original.advance(4.0);

    let saved = original.save_state().unwrap();
    let mut restored = Vm::new();
    restored.load_bytes(&script).unwrap();
    restored.load_state(&saved).unwrap();
    restored.advance(0.99);
    restored.run(100);
    assert_eq!(restored.thread_result(thread), None);
    restored.advance(0.02);
    restored.run(100);
    assert_eq!(restored.thread_result(thread), Some(Value::Int(1)));
    restored.advance(10.0);
    restored.run(100);
    assert_eq!(restored.thread_result(thread), Some(Value::Int(1)));
}
