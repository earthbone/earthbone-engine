#![no_main]

//! Grammar driven PEX fuzzing, the metamorphic half.
//!
//! `fuzz-pex` feeds raw bytes to the parser and is good at finding the
//! failures a hostile file causes. It is bad at reaching the parser's happy
//! path, because random bytes almost never form a loadable script, so the
//! agreement between the reader and the writer goes untested by it.
//!
//! This target builds a script from a grammar instead, writes it, parses it
//! back and writes it again. Any difference between the two serialisations is
//! a real disagreement: one side is losing or inventing a field. That is a
//! defect even when nothing panics, which is what makes this worth running
//! next to the byte target rather than instead of it.

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soe_papyrus::asm::{self, b, f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{self, ByteOrder, Opcode, Operand};

const LOCALS: u8 = 4;
const MAX_FUNCTIONS: usize = 4;
const MAX_INSTRUCTIONS: usize = 16;
const MAX_MEMBERS: usize = 6;

#[derive(Arbitrary, Debug)]
enum Kind {
    Int,
    Float,
    Bool,
    Text,
}

impl Kind {
    fn name(&self) -> &'static str {
        match self {
            Kind::Int => "Int",
            Kind::Float => "Float",
            Kind::Bool => "Bool",
            Kind::Text => "String",
        }
    }

    fn default_operand(&self) -> Operand {
        match self {
            Kind::Int => i(0),
            Kind::Float => f(0.0),
            Kind::Bool => b(false),
            Kind::Text => none(),
        }
    }
}

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
struct Func {
    name: u8,
    body: Vec<Ins>,
}

#[derive(Arbitrary, Debug)]
struct Plan {
    big_endian: bool,
    variables: Vec<(u8, Kind)>,
    properties: Vec<(u8, Kind)>,
    functions: Vec<Func>,
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

fuzz_target!(|plan: Plan| {
    let order = if plan.big_endian {
        ByteOrder::Big
    } else {
        ByteOrder::Little
    };
    let mut script = ScriptBuilder::new("Fuzz", "").byte_order(order);

    for (name, kind) in plan.variables.iter().take(MAX_MEMBERS) {
        let ident_name = format!("::var{name}");
        script = script.variable(&ident_name, kind.name(), kind.default_operand());
    }
    for (name, kind) in plan.properties.iter().take(MAX_MEMBERS) {
        let ident_name = format!("Prop{name}");
        script = script.auto_property(&ident_name, kind.name(), kind.default_operand());
    }

    for func in plan.functions.iter().take(MAX_FUNCTIONS) {
        let name = format!("Fn{}", func.name);
        let mut builder = FunctionBuilder::new(&name, "Int");
        for n in 0..LOCALS {
            let local = format!("v{n}");
            builder = builder.local(&local, "Int");
        }
        let mut lines = Vec::new();
        for (at, instruction) in func.body.iter().take(MAX_INSTRUCTIONS).enumerate() {
            let Some(op) = Opcode::from_u8(instruction.op % Opcode::COUNT) else {
                continue;
            };
            let args = instruction.args.iter().take(8).map(operand).collect();
            builder = builder.ins(op, args);
            lines.push(at as u16);
        }
        script = script.function(builder.lines(&lines).build());
    }

    let first = script.bytes();
    let Ok(parsed) = pex::parse(&first) else {
        // The assembler is allowed to emit a shape the reader refuses. That
        // pair is worth knowing about but it is not this target's business,
        // and treating it as a crash would drown the real signal.
        return;
    };
    let second = asm::write(&parsed);
    assert_eq!(
        first, second,
        "the reader and the writer disagree about a script the assembler produced"
    );
});
