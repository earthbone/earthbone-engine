// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Deadline yielding retains stack identity and a round-robin continuation.

use soe_papyrus::asm::{i, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Value, Vm};
use std::time::{Duration, Instant};

#[test]
fn prf_37_budget_yields_to_other_stacks_and_resumes_retained_continuations() {
    let script = ScriptBuilder::new("Busy", "")
        .function(
            FunctionBuilder::new("Loop", "None")
                .global()
                .ins(Opcode::Jmp, vec![i(0)])
                .build(),
        )
        .function(
            FunctionBuilder::new("Return", "None")
                .global()
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&script).unwrap();
    let busy = vm.call_global("Busy", "Loop", &[]).unwrap();
    let returns = vm.call_global("Busy", "Return", &[]).unwrap();
    let mut next = None;
    let before = vm.save_state().unwrap();
    assert_eq!(vm.run_before(100_000, Instant::now(), &mut next), 0);
    assert_eq!(vm.save_state().unwrap(), before);
    assert_eq!(
        vm.run_before(1, Instant::now() + Duration::from_secs(1), &mut next),
        1
    );
    assert_eq!(next, Some(returns));
    vm.run_before(10, Instant::now() + Duration::from_secs(1), &mut next);
    assert_eq!(vm.thread_result(returns), Some(Value::None));
    assert!(vm.thread_result(busy).is_none());
}
