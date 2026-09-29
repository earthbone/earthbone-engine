// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-59 suspension and recovery edge checks.

use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, ObjectId, ThreadState, Value, Vm};

fn pex(extra: bool) -> Vec<u8> {
    let mut builder = ScriptBuilder::new("Recoverable", "")
        .variable("count", "Int", i(3))
        .variable("left", "Int[]", none())
        .variable("right", "Int[]", none());
    if extra {
        builder = builder.variable("newSchema", "Int", i(0));
    }
    builder = builder
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .local("result", "None")
                .ins(
                    soe_papyrus::Opcode::CallStatic,
                    vec![s("Utility"), s("Wait"), ident("result"), i(1)],
                )
                .ins(
                    soe_papyrus::Opcode::CallStatic,
                    vec![s("Recoverable"), s("Inner"), ident("result")],
                )
                .ins(soe_papyrus::Opcode::Return, vec![none()])
                .build(),
        )
        .function(
            FunctionBuilder::new("Inner", "None")
                .ins(soe_papyrus::Opcode::Return, vec![none()])
                .build(),
        );
    builder.bytes()
}

fn source_state() -> (Vec<u8>, soe_papyrus::ThreadId, ObjectId) {
    let mut vm = Vm::new();
    vm.load_bytes(&pex(false)).unwrap();
    let object = vm.create_object("saved");
    vm.attach(object, "Recoverable").unwrap();
    let shared = ArrayRef::new(ElementKind::Int, 2);
    vm.set_var(object, "Recoverable", "left", Value::Array(shared.clone()))
        .unwrap();
    vm.set_var(object, "Recoverable", "right", Value::Array(shared))
        .unwrap();
    let thread = vm.call_global("Recoverable", "Run", &[]).unwrap();
    vm.run_until_idle(16);
    assert_eq!(vm.thread_state(thread), Some(ThreadState::Sleeping(1.0)));
    (vm.save_state().unwrap(), thread, object)
}

#[test]
fn sav_59_missing_script_state_stays_inactive_for_two_cycles_then_resumes() {
    let (saved, thread, object) = source_state();
    let mut absent = Vm::new();
    absent.load_state(&saved).unwrap();
    assert!(absent.has_suspended_state());
    assert_eq!(absent.run(100), 0);
    let cycle_one = absent.save_state().unwrap();
    let mut absent_again = Vm::new();
    absent_again.load_state(&cycle_one).unwrap();
    assert!(absent_again.has_suspended_state());
    assert_eq!(absent_again.run(100), 0);
    let cycle_two = absent_again.save_state().unwrap();
    assert_eq!(cycle_two, saved);
    absent_again.load_bytes(&pex(false)).unwrap();
    assert!(!absent_again.has_suspended_state());
    assert_eq!(
        absent_again.thread_state(thread),
        Some(ThreadState::Sleeping(1.0))
    );
    let Value::Array(left) = absent_again.get_var(object, "Recoverable", "left").unwrap() else {
        panic!("left array missing")
    };
    let Value::Array(right) = absent_again
        .get_var(object, "Recoverable", "right")
        .unwrap()
    else {
        panic!("right array missing")
    };
    assert!(std::rc::Rc::ptr_eq(&left.items, &right.items));
}

#[test]
fn sav_59_changed_pex_fingerprint_cannot_activate_saved_frames() {
    let (saved, thread, _) = source_state();
    let mut recovering = Vm::new();
    recovering.load_state(&saved).unwrap();
    recovering.load_bytes(&pex(true)).unwrap();
    assert!(recovering.has_suspended_state());
    assert_eq!(recovering.run(100), 0);
    let cycle_one = recovering.save_state().unwrap();
    let mut incompatible_again = Vm::new();
    incompatible_again.load_state(&cycle_one).unwrap();
    assert!(incompatible_again.has_suspended_state());
    assert_eq!(incompatible_again.run(100), 0);
    let cycle_two = incompatible_again.save_state().unwrap();
    let mut recovered = Vm::new();
    recovered.load_state(&cycle_two).unwrap();
    recovered.load_bytes(&pex(false)).unwrap();
    assert!(!recovered.has_suspended_state());
    assert_eq!(
        recovered.thread_state(thread),
        Some(ThreadState::Sleeping(1.0))
    );
}
