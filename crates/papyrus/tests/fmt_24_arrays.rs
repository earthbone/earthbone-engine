// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::*;
use soe_papyrus::{Value, Vm};

fn vm_with(script: &[u8]) -> Vm {
    let mut vm = Vm::new();
    vm.load_bytes(script).expect("script loads");
    vm
}

#[test]
fn fmt_24_array_mutations_preserve_identity_and_order() {
    let function = FunctionBuilder::new("Mutate", "Int")
        .local("items", "Int[]")
        .local("alias", "Int[]")
        .local("result", "Int")
        .local("value", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(2)])
        .ins(ArraySetElement, vec![ident("items"), i(0), i(1)])
        .ins(ArraySetElement, vec![ident("items"), i(1), i(2)])
        .ins(Assign, vec![ident("alias"), ident("items")])
        .ins(ArrayAdd, vec![ident("alias"), i(3), i(2)])
        .ins(ArrayInsert, vec![ident("alias"), i(9), i(1)])
        .ins(ArrayRemove, vec![ident("alias"), i(2), i(2)])
        .ins(ArrayRemoveLast, vec![ident("alias")])
        .ins(ArrayLength, vec![ident("result"), ident("items")])
        .ins(IMul, vec![ident("result"), ident("result"), i(100)])
        .ins(ArrayGetElement, vec![ident("value"), ident("alias"), i(0)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(ArrayGetElement, vec![ident("value"), ident("items"), i(1)])
        .ins(IMul, vec![ident("value"), ident("value"), i(10)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(ArrayClear, vec![ident("alias")])
        .ins(ArrayLength, vec![ident("alias"), ident("items")])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("alias")])
        .ins(Return, vec![ident("result")])
        .build();
    let mut vm = vm_with(&ScriptBuilder::new("Fmt24", "").function(function).bytes());
    let thread = vm.call_global("Fmt24", "Mutate", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    // [1, 9, 2, 3, 3] becomes [1, 9, 3], then RemoveLast gives [1, 9].
    assert_eq!(vm.thread_result(thread), Some(Value::Int(291)));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn fmt_24_array_mutations_check_bounds_and_empty_cases() {
    let function = FunctionBuilder::new("Bounds", "Int")
        .local("items", "Int[]")
        .local("alias", "Int[]")
        .local("len", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(1)])
        .ins(ArraySetElement, vec![ident("items"), i(0), i(7)])
        .ins(Assign, vec![ident("alias"), ident("items")])
        .ins(ArrayAdd, vec![ident("alias"), i(8), i(0)])
        .ins(ArrayAdd, vec![ident("alias"), i(8), i(-1)])
        .ins(ArrayRemove, vec![ident("alias"), i(0), i(0)])
        .ins(ArrayRemove, vec![ident("alias"), i(0), i(-1)])
        .ins(ArrayInsert, vec![ident("alias"), i(9), i(-1)])
        .ins(ArrayInsert, vec![ident("alias"), i(9), i(2)])
        .ins(ArrayRemove, vec![ident("alias"), i(-1), i(1)])
        .ins(ArrayRemove, vec![ident("alias"), i(2), i(1)])
        .ins(ArrayRemoveLast, vec![ident("alias")])
        .ins(ArrayRemoveLast, vec![ident("alias")])
        .ins(ArrayLength, vec![ident("len"), ident("items")])
        .ins(Return, vec![ident("len")])
        .build();
    let mut vm = vm_with(&ScriptBuilder::new("Fmt24", "").function(function).bytes());
    let thread = vm.call_global("Fmt24", "Bounds", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(0)));
    let messages = vm.log.messages().join("\n");
    assert!(
        messages.contains("array insert index -1 out of range"),
        "{messages}"
    );
    assert!(
        messages.contains("array insert index 2 out of range"),
        "{messages}"
    );
    assert!(
        messages.contains("array remove index -1 out of range"),
        "{messages}"
    );
    assert!(
        messages.contains("array remove index 2 out of range"),
        "{messages}"
    );
}

#[test]
fn fmt_24_array_mutation_continuation_restores_shared_array() {
    let function = FunctionBuilder::new("Continue", "Int")
        .local("items", "Int[]")
        .local("alias", "Int[]")
        .local("result", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(1)])
        .ins(ArraySetElement, vec![ident("items"), i(0), i(7)])
        .ins(Assign, vec![ident("alias"), ident("items")])
        .ins(ArrayAdd, vec![ident("alias"), i(8), i(2)])
        .ins(ArrayInsert, vec![ident("alias"), i(9), i(1)])
        .ins(ArrayRemove, vec![ident("alias"), i(2), i(1)])
        .ins(ArrayRemoveLast, vec![ident("alias")])
        .ins(ArraySetElement, vec![ident("alias"), i(0), i(12)])
        .ins(ArrayGetElement, vec![ident("result"), ident("items"), i(0)])
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Fmt24", "").function(function).bytes();
    let mut uninterrupted = vm_with(&bytes);
    let thread = uninterrupted.call_global("Fmt24", "Continue", &[]).unwrap();
    assert_eq!(uninterrupted.run(7), 7);
    let saved = uninterrupted.save_state().unwrap();
    let mut resumed = vm_with(&bytes);
    resumed.load_state(&saved).unwrap();
    assert!(resumed.run_until_idle(1000));
    assert_eq!(resumed.thread_result(thread), Some(Value::Int(12)));
    assert!(uninterrupted.run_until_idle(1000));
    assert_eq!(uninterrupted.thread_result(thread), Some(Value::Int(12)));
    assert_eq!(
        resumed.save_state().unwrap(),
        uninterrupted.save_state().unwrap()
    );
}
