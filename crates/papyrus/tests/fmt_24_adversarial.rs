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
fn insert_at_empty_and_end_and_remove_past_tail_keep_alias_live() {
    let function = FunctionBuilder::new("Edges", "Int")
        .local("items", "Int[]")
        .local("alias", "Int[]")
        .local("result", "Int")
        .local("value", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(0)])
        .ins(Assign, vec![ident("alias"), ident("items")])
        .ins(ArrayInsert, vec![ident("alias"), i(4), i(0)])
        .ins(ArrayInsert, vec![ident("alias"), i(5), i(1)])
        .ins(ArrayAdd, vec![ident("alias"), i(6), i(2)])
        .ins(ArrayRemove, vec![ident("items"), i(2), i(i32::MAX)])
        .ins(ArrayLength, vec![ident("result"), ident("alias")])
        .ins(IMul, vec![ident("result"), ident("result"), i(100)])
        .ins(ArrayGetElement, vec![ident("value"), ident("items"), i(0)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(ArrayGetElement, vec![ident("value"), ident("alias"), i(1)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Fmt24Edges", "")
        .function(function)
        .bytes();
    let mut vm = vm_with(&bytes);
    let thread = vm.call_global("Fmt24Edges", "Edges", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(209)));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn invalid_insert_and_remove_leave_every_element_unchanged() {
    let function = FunctionBuilder::new("Invalid", "Int")
        .local("items", "Int[]")
        .local("result", "Int")
        .local("value", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(2)])
        .ins(ArraySetElement, vec![ident("items"), i(0), i(10)])
        .ins(ArraySetElement, vec![ident("items"), i(1), i(20)])
        .ins(ArrayInsert, vec![ident("items"), i(99), i(3)])
        .ins(ArrayRemove, vec![ident("items"), i(-1), i(1)])
        .ins(ArrayRemove, vec![ident("items"), i(2), i(1)])
        .ins(ArrayLength, vec![ident("result"), ident("items")])
        .ins(IMul, vec![ident("result"), ident("result"), i(100)])
        .ins(ArrayGetElement, vec![ident("value"), ident("items"), i(0)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(IMul, vec![ident("result"), ident("result"), i(10)])
        .ins(ArrayGetElement, vec![ident("value"), ident("items"), i(1)])
        .ins(IAdd, vec![ident("result"), ident("result"), ident("value")])
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Fmt24Invalid", "")
        .function(function)
        .bytes();
    let mut vm = vm_with(&bytes);
    let thread = vm.call_global("Fmt24Invalid", "Invalid", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(2120)));
    let messages = vm.log.messages().join("\n");
    assert!(
        messages.contains("array insert index 3 out of range"),
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
fn add_and_insert_at_array_limit_leave_length_unchanged() {
    let function = FunctionBuilder::new("ArrayLimit", "Int")
        .local("items", "Int[]")
        .local("length", "Int")
        .ins(ArrayCreate, vec![ident("items"), i(1 << 20)])
        .ins(ArrayAdd, vec![ident("items"), i(7), i(1)])
        .ins(ArrayInsert, vec![ident("items"), i(7), i(1 << 20)])
        .ins(ArrayLength, vec![ident("length"), ident("items")])
        .ins(Return, vec![ident("length")])
        .build();
    let bytes = ScriptBuilder::new("Fmt24Limit", "")
        .function(function)
        .bytes();
    let mut vm = vm_with(&bytes);
    let thread = vm.call_global("Fmt24Limit", "ArrayLimit", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(1 << 20)));
    let messages = vm.log.messages().join("\n");
    assert!(
        messages.contains("array length limit exceeded by Add"),
        "{messages}"
    );
    assert!(
        messages.contains("array length limit exceeded by Insert"),
        "{messages}"
    );
}

#[test]
fn all_mutations_on_none_arrays_log_and_continue() {
    let function = FunctionBuilder::new("NoneArray", "Int")
        .local("items", "Int[]")
        .local("result", "Int")
        .ins(ArrayAdd, vec![ident("items"), i(1), i(1)])
        .ins(ArrayInsert, vec![ident("items"), i(1), i(0)])
        .ins(ArrayRemoveLast, vec![ident("items")])
        .ins(ArrayRemove, vec![ident("items"), i(0), i(1)])
        .ins(ArrayClear, vec![ident("items")])
        .ins(Assign, vec![ident("result"), i(17)])
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Fmt24None", "")
        .function(function)
        .bytes();
    let mut vm = vm_with(&bytes);
    let thread = vm.call_global("Fmt24None", "NoneArray", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(17)));
    let messages = vm.log.messages().join("\n");
    for message in [
        "cannot add to a None array",
        "cannot insert into a None array",
        "cannot remove from a None array",
        "cannot clear a None array",
    ] {
        assert!(messages.contains(message), "missing {message}: {messages}");
    }
}
