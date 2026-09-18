// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A script cannot hold two functions of one name in one state, and the
//! assembler must not build one.
//!
//! `fuzz-pex-grammar` found this as a reader and writer disagreement: a script
//! written with two same named functions comes back different. The debug block
//! is where it shows, because `attach_lines` in the reader matches a debug
//! entry to a function by name and `find` always returns the first, so the
//! second function is left with no lines and the rewrite emits one debug entry
//! where the first emitted two.
//!
//! The reader is right and the assembler was wrong. Papyrus has no overloading
//! and a state defines each function once, so two same named functions in one
//! state is a file no compiler emits and the format cannot express. The
//! assembler now replaces rather than appends, which is the same stance
//! `FunctionBuilder::ins` already takes when it truncates operands the writer
//! would not emit.

use soe_papyrus::asm::{self, i, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode};

fn script_with_two_functions_named(name: &str) -> Vec<u8> {
    let first = FunctionBuilder::new(name, "Int")
        .ins(Opcode::Return, vec![i(1)])
        .lines(&[10])
        .build();
    let second = FunctionBuilder::new(name, "Int")
        .ins(Opcode::Return, vec![i(2)])
        .lines(&[20])
        .build();
    ScriptBuilder::new("Dup", "")
        .function(first)
        .function(second)
        .bytes()
}

#[test]
fn a_repeated_function_name_replaces_rather_than_duplicating() {
    let bytes = script_with_two_functions_named("Go");
    let parsed = pex::parse(&bytes).expect("the script parses");
    let state = &parsed.objects[0].states[0];
    let named: Vec<&str> = state
        .functions
        .iter()
        .filter(|f| f.name.eq_ignore_ascii_case("Go"))
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(
        named.len(),
        1,
        "one state cannot define `Go` twice, Papyrus has no overloading"
    );
    let go = state
        .functions
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("Go"))
        .expect("the function survives");
    assert_eq!(go.lines, vec![20], "the later definition wins");
    assert_eq!(go.code[0].args[0], i(2), "and carries its own body");
}

#[test]
fn the_round_trip_is_exact_for_a_repeated_name() {
    // This is the property `fuzz-pex-grammar` asserts, on the minimal input
    // that used to break it. Repro:
    //   cargo test -p soe-papyrus --test asm_unique_names
    let first = script_with_two_functions_named("Go");
    let parsed = pex::parse(&first).expect("the script parses");
    let second = asm::write(&parsed);
    assert_eq!(
        first, second,
        "the reader and the writer disagree about a script the assembler produced"
    );
}

#[test]
fn distinct_names_are_untouched() {
    let a = FunctionBuilder::new("A", "Int")
        .ins(Opcode::Return, vec![i(1)])
        .lines(&[1])
        .build();
    let b = FunctionBuilder::new("B", "Int")
        .ins(Opcode::Return, vec![i(2)])
        .lines(&[2])
        .build();
    let bytes = ScriptBuilder::new("Two", "")
        .function(a)
        .function(b)
        .bytes();
    let parsed = pex::parse(&bytes).expect("the script parses");
    assert_eq!(parsed.objects[0].states[0].functions.len(), 2);
    assert_eq!(bytes, asm::write(&parsed));
}
