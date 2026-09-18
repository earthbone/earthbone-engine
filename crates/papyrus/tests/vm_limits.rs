// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Regression tests for a script asking the VM for more than the machine has.
//!
//! Found by `fuzz-vm-run` (`TEST_LOG.md` entry 2) on its second run: a single
//! `ArrayCreate` whose size operand was a large positive integer asked for
//! 26.5 GB in one allocation and took the process down. The negative case was
//! already handled, the large positive one was not.
//!
//! This is not an invented hazard. The Creation Kit wiki warns authors about
//! it directly, in `Utility_Script.wikitext`: "It is possible to create arrays
//! longer than 128 elements, and size is treated as an unsigned integer.
//! Negative numbers will produce extremely large values, which can cause major
//! issues." Vanilla asks the author to be careful. This engine refuses
//! instead, which is the same trade the rest of the project makes: replace the
//! workaround with a system that cannot go wrong that way.

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::{
    ArrayCreate, ArrayLength, Assign, CmpLt, IAdd, Jmp, JmpF, Return, StrCat,
};
use soe_papyrus::{Value, Vm};

/// Builds a script that creates an array of `size` and returns its length.
fn array_script(size: i32) -> Vec<u8> {
    let make = FunctionBuilder::new("Make", "Int")
        .local("arr", "Int[]")
        .local("n", "Int")
        .ins(ArrayCreate, vec![ident("arr"), i(size)])
        .ins(ArrayLength, vec![ident("n"), ident("arr")])
        .ins(Return, vec![ident("n")])
        .lines(&[1, 2, 3])
        .build();
    ScriptBuilder::new("Arrays", "").function(make).bytes()
}

fn run(size: i32) -> (Vm, Value) {
    let mut vm = Vm::new();
    vm.load_bytes(&array_script(size)).expect("script loads");
    let tid = vm.call_global("Arrays", "Make", &[]).unwrap();
    assert!(vm.run_until_idle(10_000), "the VM should go idle");
    let result = vm.thread_result(tid).unwrap_or(Value::None);
    (vm, result)
}

#[test]
fn an_array_of_a_reasonable_size_is_created() {
    let (vm, result) = run(128);
    assert_eq!(result, Value::Int(128));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn an_array_larger_than_the_limit_is_refused_rather_than_allocated() {
    // The size the fuzzer reached was around a billion. Anything past the cap
    // takes the same path, so the test uses a number a reader recognises as
    // absurd rather than the exact one libFuzzer happened to find.
    let (vm, result) = run(1_000_000_000);
    // The destination is left as None, so the length that comes back is zero.
    // The refusal itself is the log entry, which is what has to be there.
    assert_eq!(result, Value::Int(0), "a refused array must not come back");
    let messages = vm.log.messages().join("\n");
    assert!(
        messages.contains("array size") && messages.contains("limit"),
        "the refusal should name the size and the limit, got {messages:?}"
    );
}

#[test]
fn a_negative_array_size_is_still_refused() {
    let (vm, result) = run(-1);
    assert_eq!(result, Value::Int(0));
    let messages = vm.log.messages().join("\n");
    assert!(messages.contains("array size below zero"), "{messages:?}");
}

/// Doubles a string in a loop, the shape the fuzzer reached. Each pass makes
/// the string twice as long, so the step budget never binds: thirty passes
/// from eight bytes is already gigabytes.
fn doubling_script(passes: i32) -> Vec<u8> {
    let grow = FunctionBuilder::new("Grow", "Int")
        .local("txt", "String")
        .local("n", "Int")
        .local("go", "Bool")
        .ins(Assign, vec![ident("txt"), s("abcdefgh")])
        .ins(Assign, vec![ident("n"), i(0)])
        .ins(CmpLt, vec![ident("go"), ident("n"), i(passes)])
        .ins(JmpF, vec![ident("go"), i(4)])
        .ins(StrCat, vec![ident("txt"), ident("txt"), ident("txt")])
        .ins(IAdd, vec![ident("n"), ident("n"), i(1)])
        .ins(Jmp, vec![i(-4)])
        .ins(Return, vec![ident("n")])
        .lines(&[1, 2, 3, 4, 5, 6, 7, 8])
        .build();
    ScriptBuilder::new("Strings", "").function(grow).bytes()
}

#[test]
fn a_string_doubled_in_a_loop_is_refused_rather_than_grown() {
    let mut vm = Vm::new();
    vm.load_bytes(&doubling_script(40)).expect("script loads");
    let tid = vm.call_global("Strings", "Grow", &[]).unwrap();
    assert!(vm.run_until_idle(10_000), "the VM should go idle");
    assert_eq!(vm.thread_result(tid), Some(Value::Int(40)));
    let messages = vm.log.messages().join("\n");
    assert!(
        messages.contains("string length"),
        "the refusal should name the length, got {messages:?}"
    );
}

#[test]
fn a_short_concatenation_still_works() {
    let mut vm = Vm::new();
    vm.load_bytes(&doubling_script(4)).expect("script loads");
    let tid = vm.call_global("Strings", "Grow", &[]).unwrap();
    assert!(vm.run_until_idle(10_000), "the VM should go idle");
    assert_eq!(vm.thread_result(tid), Some(Value::Int(4)));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}

#[test]
fn the_limit_itself_is_allocatable() {
    // A cap nobody can reach is a cap that hides a smaller real limit. This
    // pins that the documented number is the number that works.
    let (vm, result) = run(soe_papyrus::MAX_ARRAY_LEN as i32);
    assert_eq!(result, Value::Int(soe_papyrus::MAX_ARRAY_LEN as i32));
    assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
}
