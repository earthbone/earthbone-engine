#![no_main]

//! Structure aware fuzzing of the VM, not of the loader.
//!
//! `fuzz-pex` throws bytes at the parser and finds parser bugs. It almost
//! never produces a script that loads, so the interpreter behind it stays
//! unfuzzed. This target starts from a valid script every time: `arbitrary`
//! picks opcodes and operands, the assembler writes real PEX, and the VM runs
//! what comes back.
//!
//! What counts as a failure here is a panic or a hang, not an error. A
//! generated program is usually nonsense (a jump into the middle of nothing,
//! an add over a string) and `Err` is the right answer for all of it, so the
//! target ignores every `Result`. Two budgets keep a hostile program from
//! looking like a hang: the instruction count the assembler will emit, and the
//! step budget `run_until_idle` enforces.

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soe_papyrus::asm::{b, f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{Opcode, Operand};
use soe_papyrus::Vm;

/// Enough to build a loop and a few nested calls, small enough that libFuzzer
/// explores shapes rather than lengths.
const MAX_INSTRUCTIONS: usize = 48;
/// Locals are named `v0` upward and every local operand is taken modulo this,
/// so an operand always names a declared variable.
const LOCALS: u8 = 6;
/// The VM stops here. A generated `Jmp -1` is an infinite loop by design, and
/// stopping on the budget is the correct answer rather than a finding.
const STEPS: usize = 20_000;

#[derive(Arbitrary, Debug)]
enum Arg {
    Local(u8),
    Int(i32),
    Float(f32),
    Bool(bool),
    Text(u8),
    Nothing,
}

#[derive(Arbitrary, Debug)]
struct Ins {
    op: u8,
    args: Vec<Arg>,
}

#[derive(Arbitrary, Debug)]
struct Program {
    body: Vec<Ins>,
}

fn operand(arg: &Arg) -> Operand {
    match arg {
        Arg::Local(n) => {
            let name = format!("v{}", n % LOCALS);
            ident(&name)
        }
        Arg::Int(v) => i(*v),
        Arg::Float(v) => f(*v),
        Arg::Bool(v) => b(*v),
        Arg::Text(n) => {
            let text = format!("s{n}");
            s(&text)
        }
        Arg::Nothing => none(),
    }
}

fuzz_target!(|program: Program| {
    if program.body.is_empty() {
        return;
    }

    let mut function = FunctionBuilder::new("Run", "Int");
    for n in 0..LOCALS {
        let name = format!("v{n}");
        function = function.local(&name, "Int");
    }
    let mut lines = Vec::new();
    for (at, instruction) in program.body.iter().take(MAX_INSTRUCTIONS).enumerate() {
        let Some(op) = Opcode::from_u8(instruction.op % Opcode::COUNT) else {
            continue;
        };
        // A long operand list is a shape the assembler has to survive, but an
        // unbounded one only makes the case slow to write and to shrink.
        let args = instruction.args.iter().take(8).map(operand).collect();
        function = function.ins(op, args);
        lines.push(at as u16);
    }
    let bytes = ScriptBuilder::new("Fuzz", "")
        .function(function.lines(&lines).build())
        .bytes();

    // The assembler may refuse a shape, and the loader may refuse what it
    // wrote. Both are answers, not failures.
    let mut vm = Vm::new();
    if vm.load_bytes(&bytes).is_err() {
        return;
    }
    if vm.call_global("Fuzz", "Run", &[]).is_err() {
        return;
    }
    let _ = vm.run_until_idle(STEPS);
});
