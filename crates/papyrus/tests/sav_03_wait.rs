// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{Opcode, Value, Vm};

fn sleeping_script() -> Vec<u8> {
    ScriptBuilder::new("Sav03Wait", "")
        .variable("::fired", "Int", i(0))
        .function(
            FunctionBuilder::new("Run", "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s("Wait"), ident("::nonevar"), f(5.0)],
                )
                .ins(Opcode::IAdd, vec![ident("::fired"), ident("::fired"), i(1)])
                .ins(Opcode::Return, vec![ident("::fired")])
                .build(),
        )
        .bytes()
}

#[test]
fn sav_03_saved_utility_wait_resumes_after_only_remaining_delay() {
    let script = sleeping_script();
    let mut original = Vm::new();
    original.load_bytes(&script).unwrap();
    let object = original.create_object("wait owner");
    original.attach(object, "Sav03Wait").unwrap();
    let thread = original.call(object, "Run", &[]).unwrap();
    original.run(100);
    assert!(matches!(
        original.thread_state(thread),
        Some(ThreadState::Sleeping(_))
    ));

    original.advance(2.0);
    let saved = original.save_state().unwrap();

    let mut restored = Vm::new();
    restored.load_bytes(&script).unwrap();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.thread_state(thread), original.thread_state(thread));
    assert_eq!(
        restored.get_var(object, "Sav03Wait", "::fired"),
        Some(Value::Int(0))
    );

    restored.advance(2.99);
    restored.run(100);
    assert_eq!(
        restored.get_var(object, "Sav03Wait", "::fired"),
        Some(Value::Int(0))
    );
    assert_eq!(restored.thread_result(thread), None);

    restored.advance(0.02);
    restored.run(100);
    assert_eq!(
        restored.get_var(object, "Sav03Wait", "::fired"),
        Some(Value::Int(1))
    );
    assert_eq!(restored.thread_result(thread), Some(Value::Int(1)));

    restored.advance(10.0);
    restored.run(100);
    assert_eq!(
        restored.get_var(object, "Sav03Wait", "::fired"),
        Some(Value::Int(1))
    );
    assert_eq!(restored.thread_result(thread), Some(Value::Int(1)));
}
