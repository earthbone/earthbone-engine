// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::{ThreadState, Value, Vm, MAX_COMPLETED_THREAD_HISTORY};

#[test]
fn prf_01_late_completion_is_retained_and_survives_save_load() {
    let script = ScriptBuilder::new("RetentionOrder", "")
        .function(
            FunctionBuilder::new("Late", "Int")
                .global()
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("Wait"), ident("::nonevar"), f(1.0)],
                )
                .ins(Return, vec![i(99)])
                .build(),
        )
        .function(
            FunctionBuilder::new("Quick", "Int")
                .global()
                .ins(Return, vec![i(7)])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&script).unwrap();

    let late = vm.call_global("RetentionOrder", "Late", &[]).unwrap();
    vm.run(MAX_COMPLETED_THREAD_HISTORY * 4);
    assert!(matches!(
        vm.thread_state(late),
        Some(ThreadState::Sleeping(_))
    ));

    for _ in 0..=MAX_COMPLETED_THREAD_HISTORY {
        vm.call_global("RetentionOrder", "Quick", &[]).unwrap();
    }
    vm.run(MAX_COMPLETED_THREAD_HISTORY * 4);
    vm.advance(1.0);
    vm.run(16);

    assert_eq!(vm.thread_state(late), Some(ThreadState::Done));
    assert_eq!(vm.thread_result(late), Some(Value::Int(99)));

    let saved = vm.save_state().unwrap();
    let mut restored = Vm::new();
    restored.load_bytes(&script).unwrap();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.thread_state(late), Some(ThreadState::Done));
    assert_eq!(restored.thread_result(late), Some(Value::Int(99)));
}
