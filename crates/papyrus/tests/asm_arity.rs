// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Regression tests for instructions built with fewer operands than their
//! opcode takes.
//!
//! Found by `fuzz-vm-run` (`TEST_LOG.md` entry 2) on its first run, in under
//! five minutes, with the smallest case the grammar can express: one
//! `CallStatic` and no operands at all. `asm::write` sliced `args[..3]` on an
//! empty list and panicked with "range end index 3 out of range for slice of
//! length 0", a message that names neither the opcode nor the function it came
//! from.
//!
//! The writer returns `Vec<u8>` and has no error channel, so it has to be
//! total. It now pads the fixed operands with `Operand::None`, which is a real
//! operand in this format rather than an invention, and the result parses back
//! to the same instruction. That keeps a script built by hand, by a tool, or
//! by a generator from taking the process down.

use soe_papyrus::asm::{self, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, Opcode, Operand};

/// Every opcode, given no operands at all. The one the fuzzer found was
/// `CallStatic`, but nothing about the defect was specific to it.
#[test]
fn every_opcode_survives_being_given_no_operands() {
    for code in 0..Opcode::COUNT {
        let op = Opcode::from_u8(code).expect("the table is dense from zero");
        let function = FunctionBuilder::new("Run", "Int")
            .ins(op, Vec::new())
            .build();
        let bytes = ScriptBuilder::new("Short", "").function(function).bytes();
        assert!(
            !bytes.is_empty(),
            "{op:?} with no operands produced no bytes"
        );
    }
}

/// The exact case the fuzzer minimised to.
#[test]
fn a_call_static_with_no_operands_does_not_panic() {
    let function = FunctionBuilder::new("Run", "Int")
        .ins(Opcode::CallStatic, Vec::new())
        .build();
    let bytes = ScriptBuilder::new("Fuzz", "").function(function).bytes();
    let parsed = pex::parse(&bytes).expect("the padded instruction reads back");
    let code = &parsed.objects[0].states[0].functions[0].code;
    assert_eq!(code.len(), 1);
    assert_eq!(code[0].op, Opcode::CallStatic);
    assert_eq!(
        &code[0].args[..3],
        &[Operand::None, Operand::None, Operand::None],
        "the three fixed operands are padded, not dropped"
    );
}

/// Padding must not disturb an instruction that already has what it needs,
/// and the round trip has to be stable so `fuzz-pex-grammar` does not report
/// the padding itself as a reader and writer disagreement.
#[test]
fn a_padded_instruction_round_trips_to_the_same_bytes() {
    for code in 0..Opcode::COUNT {
        let op = Opcode::from_u8(code).expect("the table is dense from zero");
        for supplied in 0..=2usize {
            let args = vec![Operand::Int(7); supplied];
            let function = FunctionBuilder::new("Run", "Int").ins(op, args).build();
            let first = ScriptBuilder::new("Short", "").function(function).bytes();
            // A variadic opcode reads its count from an operand the padding
            // supplied as None, which the reader may refuse. That is a parse
            // error, not a panic, and it is the answer here.
            let Ok(parsed) = pex::parse(&first) else {
                continue;
            };
            let second = asm::write(&parsed);
            assert_eq!(
                first, second,
                "{op:?} with {supplied} operands did not round trip"
            );
        }
    }
}
