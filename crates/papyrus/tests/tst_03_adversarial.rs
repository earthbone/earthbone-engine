// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode;
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, Value, Vm};

#[test]
fn cloned_array_handle_mutations_are_shared_but_new_handles_are_isolated() {
    let original = ArrayRef::new(ElementKind::Int, 1);
    let alias = original.clone();
    let separate = ArrayRef::new(ElementKind::Int, 1);
    original.items.borrow_mut()[0] = Value::Int(9);
    assert_eq!(alias.get(0), Some(Value::Int(9)));
    assert_eq!(separate.get(0), Some(Value::Int(0)));
}

#[test]
fn invalid_array_indices_do_not_alias_the_last_element() {
    let array = ArrayRef::new(ElementKind::Str, 1);
    assert_eq!(array.get(1), None);
    assert_eq!(array.get(usize::MAX), None);
}

#[test]
fn empty_and_nonempty_arrays_have_distinct_truth_values() {
    assert!(!Value::Array(ArrayRef::new(ElementKind::Bool, 0)).as_bool());
    assert!(Value::Array(ArrayRef::new(ElementKind::Bool, 1)).as_bool());
}

#[test]
fn scheduler_budget_can_pause_and_resume_a_runnable_thread() {
    let mut function = FunctionBuilder::new("Count", "Int");
    for _ in 0..8 {
        function = function.ins(Opcode::Nop, Vec::new());
    }
    let bytes = ScriptBuilder::new("Budget", "")
        .function(function.ins(Opcode::Return, vec![i(7)]).build())
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    let thread = vm.call_global("Budget", "Count", &[]).unwrap();
    assert!(!vm.run_until_idle(1));
    assert!(vm.run_until_idle(32));
    assert_eq!(vm.thread_result(thread), Some(Value::Int(7)));
}

#[test]
fn conditional_jump_outside_function_terminates_with_a_diagnostic() {
    let function = FunctionBuilder::new("Jump", "Int")
        .ins(Opcode::Jmp, vec![i(99)])
        .ins(Opcode::Return, vec![i(1)])
        .build();
    let bytes = ScriptBuilder::new("BadJump", "").function(function).bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    let thread = vm.call_global("BadJump", "Jump", &[]).unwrap();
    assert!(vm.run_until_idle(8));
    assert_eq!(vm.thread_result(thread), Some(Value::None));
    assert!(vm
        .log
        .messages()
        .join("\n")
        .contains("outside the function"));
}

#[test]
fn non_strict_comparisons_include_equal_operands() {
    let function = FunctionBuilder::new("Equal", "Bool")
        .local("atMost", "Bool")
        .local("atLeast", "Bool")
        .ins(Opcode::CmpLte, vec![ident("atMost"), i(9), i(9)])
        .ins(Opcode::CmpGte, vec![ident("atLeast"), i(9), i(9)])
        .ins(Opcode::Return, vec![ident("atMost")])
        .build();
    let bytes = ScriptBuilder::new("Compare", "").function(function).bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    let thread = vm.call_global("Compare", "Equal", &[]).unwrap();
    assert!(vm.run_until_idle(8));
    assert_eq!(vm.thread_result(thread), Some(Value::Bool(true)));
}
