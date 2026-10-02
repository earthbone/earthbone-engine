// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{Opcode::*, StructDefinition, StructMember};
use soe_papyrus::{Value, Vm};

fn script(functions: Vec<FunctionBuilder>, minor: u8) -> soe_papyrus::pex::Script {
    let mut builder = ScriptBuilder::new("Breaker", "").variable("entry", "Entry", none());
    for function in functions {
        builder = builder.function(function.build());
    }
    let mut script = builder.build();
    script.header.game_id = 2;
    script.header.major = 3;
    script.header.minor = minor;
    script.objects[0].structs.push(StructDefinition {
        name: "Entry".into(),
        members: vec![
            StructMember {
                name: "count".into(),
                type_name: "Int".into(),
                user_flags: 0,
                default: i(7),
                is_const: false,
                docstring: String::new(),
            },
            StructMember {
                name: "fixed".into(),
                type_name: "Bool".into(),
                user_flags: 0,
                default: soe_papyrus::pex::Operand::Bool(true),
                is_const: true,
                docstring: String::new(),
            },
        ],
    });
    script
}

fn result(vm: &mut Vm, function: &str, args: &[Value]) -> Option<Value> {
    let tid = vm.call_global("Breaker", function, args).unwrap();
    assert!(vm.run_until_idle(1000));
    vm.thread_result(tid)
}

#[test]
fn fmt_13_is_none_is_false_and_invalid_runtime_source_is_rejected() {
    let check = FunctionBuilder::new("Check", "Bool")
        .param("subject", "Object")
        .local("matched", "Bool")
        .ins(Is, vec![ident("matched"), ident("subject"), ident("aCtOr")])
        .ins(Return, vec![ident("matched")]);
    let mut vm = Vm::new();
    vm.load(script(vec![check], 9));
    assert_eq!(
        result(&mut vm, "Check", &[Value::None]),
        Some(Value::Bool(false))
    );
    assert_eq!(
        result(&mut vm, "Check", &[Value::Int(4)]),
        Some(Value::Bool(false))
    );
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("Is operand must be an object")));
}

#[test]
fn fmt_13_is_rejects_empty_identifier_and_non_bool_destinations() {
    let empty = FunctionBuilder::new("Empty", "Bool")
        .param("subject", "Object")
        .local("matched", "Bool")
        .ins(Is, vec![ident("matched"), ident("subject"), ident("")])
        .ins(Return, vec![ident("matched")]);
    let wrong_dest = FunctionBuilder::new("WrongDest", "Int")
        .param("subject", "Object")
        .local("matched", "Int")
        .ins(Is, vec![ident("matched"), ident("subject"), ident("Actor")])
        .ins(Return, vec![ident("matched")]);
    let mut vm = Vm::new();
    vm.load(script(vec![empty, wrong_dest], 9));
    let actor = vm.create_object_of("actor", "Actor");
    assert_eq!(
        result(&mut vm, "Empty", &[Value::Object(actor)]),
        Some(Value::Bool(false))
    );
    assert_eq!(
        result(&mut vm, "WrongDest", &[Value::Object(actor)]),
        Some(Value::Int(0))
    );
    assert_eq!(
        vm.log
            .entries
            .iter()
            .filter(|entry| entry.message.contains("Is requires a Bool destination"))
            .count(),
        2
    );
}

#[test]
fn fmt_13_struct_fields_are_case_insensitive_and_preserve_rejected_writes() {
    let fields = FunctionBuilder::new("Fields", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), s("COUNT"), i(11)])
        .ins(StructSet, vec![ident("entry"), s("count"), s("wrong")])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("Count")])
        .ins(Return, vec![ident("count")]);
    let mut vm = Vm::new();
    vm.load(script(vec![fields], 9));
    assert_eq!(result(&mut vm, "Fields", &[]), Some(Value::Int(11)));
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("does not match struct member count type Int")));
}

#[test]
fn fmt_13_const_struct_field_rejects_repeated_mutation() {
    let fields = FunctionBuilder::new("Fields", "Bool")
        .local("entry", "Entry")
        .local("fixed", "Bool")
        .ins(StructCreate, vec![ident("entry")])
        .ins(
            StructSet,
            vec![ident("entry"), s("fixed"), soe_papyrus::asm::b(false)],
        )
        .ins(StructGet, vec![ident("fixed"), ident("entry"), s("fixed")])
        .ins(Return, vec![ident("fixed")]);
    let mut vm = Vm::new();
    vm.load(script(vec![fields], 9));
    assert_eq!(result(&mut vm, "Fields", &[]), Some(Value::Bool(true)));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("struct member fixed is const")));
}

#[test]
fn fmt_13_struct_opcodes_reject_pre_39_versions() {
    let fields = FunctionBuilder::new("Fields", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), s("count"), i(12)])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let mut vm = Vm::new();
    vm.load(script(vec![fields], 8));
    assert_eq!(result(&mut vm, "Fields", &[]), Some(Value::None));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("requires a Fallout 4 3.9")));
}
