// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{Opcode::*, StructDefinition, StructMember};
use soe_papyrus::{Value, Vm};

fn fo4_script(functions: Vec<FunctionBuilder>) -> soe_papyrus::pex::Script {
    let mut builder = ScriptBuilder::new("Fmt13", "")
        .variable("target", "Object", none())
        .variable("entry", "Entry", none());
    for function in functions {
        builder = builder.function(function.build());
    }
    let mut script = builder.build();
    script.header.game_id = 2;
    script.header.major = 3;
    script.header.minor = 9;
    script.objects[0].structs.push(StructDefinition {
        name: "Entry".into(),
        members: vec![StructMember {
            name: "count".into(),
            type_name: "Int".into(),
            user_flags: 0,
            default: i(7),
            is_const: false,
            docstring: String::new(),
        }],
    });
    script
}

#[test]
fn fmt_13_is_and_struct_fields_execute_with_declared_types() {
    let check = FunctionBuilder::new("Check", "Bool")
        .param("subject", "Object")
        .local("matched", "Bool")
        .ins(Is, vec![ident("matched"), ident("subject"), ident("Actor")])
        .ins(Return, vec![ident("matched")]);
    let invalid_type = FunctionBuilder::new("InvalidType", "Bool")
        .param("subject", "Object")
        .local("matched", "Bool")
        .ins(Is, vec![ident("matched"), ident("subject"), i(4)])
        .ins(Return, vec![ident("matched")]);
    let invalid_name_kind = FunctionBuilder::new("InvalidNameKind", "Bool")
        .param("subject", "Object")
        .local("matched", "Bool")
        .ins(Is, vec![ident("matched"), ident("subject"), s("Actor")])
        .ins(Return, vec![ident("matched")]);
    let invalid_destination = FunctionBuilder::new("InvalidDestination", "Int")
        .param("subject", "Object")
        .local("matched", "Int")
        .ins(Is, vec![ident("matched"), ident("subject"), ident("Actor")])
        .ins(Return, vec![ident("matched")]);
    let fields = FunctionBuilder::new("Fields", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), s("count"), i(9)])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let script = fo4_script(vec![
        check,
        invalid_type,
        invalid_name_kind,
        invalid_destination,
        fields,
    ]);
    let mut vm = Vm::new();
    vm.load(script);
    let actor = vm.create_object_of("actor", "Actor");
    let tid = vm.call_global("Fmt13", "Fields", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::Int(9)));
    let tid = vm
        .call_global("Fmt13", "Check", &[Value::Object(actor)])
        .unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::Bool(true)));
    let tid = vm.call_global("Fmt13", "Check", &[Value::Int(4)]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::Bool(false)));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("Is operand must be an object")));
    let tid = vm
        .call_global("Fmt13", "InvalidType", &[Value::Object(actor)])
        .unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::Bool(false)));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("Is requires a Bool destination")));
    for function in ["InvalidNameKind", "InvalidDestination"] {
        let tid = vm
            .call_global("Fmt13", function, &[Value::Object(actor)])
            .unwrap();
        assert!(vm.run_until_idle(1000));
        assert!(matches!(
            vm.thread_result(tid),
            Some(Value::Bool(false) | Value::Int(0))
        ));
    }
}

#[test]
fn fmt_13_invalid_struct_operands_are_logged_and_leave_fields_unchanged() {
    let function = FunctionBuilder::new("BadFields", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .local("wrong", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), s("missing"), i(9)])
        .ins(StructSet, vec![ident("wrong"), s("count"), i(9)])
        .ins(StructSet, vec![ident("entry"), s("count"), s("bad type")])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let mut vm = Vm::new();
    vm.load(fo4_script(vec![function]));
    let tid = vm.call_global("Fmt13", "BadFields", &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tid), Some(Value::Int(7)));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("no member missing")));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("must be a struct")));
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("does not match struct member count type Int")));
}

#[test]
fn fmt_13_struct_get_checks_destination_and_member_operand_types() {
    let wrong_destination = FunctionBuilder::new("WrongDestination", "Bool")
        .local("entry", "Entry")
        .local("count", "Bool")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let wrong_member_operand = FunctionBuilder::new("WrongMemberOperand", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructGet, vec![ident("count"), ident("entry"), i(0)])
        .ins(Return, vec![ident("count")]);
    let invalid_set_member_operand = FunctionBuilder::new("InvalidSetMemberOperand", "Int")
        .local("entry", "Entry")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), i(0), i(9)])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let script = fo4_script(vec![
        wrong_destination,
        wrong_member_operand,
        invalid_set_member_operand,
    ]);
    let mut vm = Vm::new();
    vm.load(script);

    for (function, expected) in [
        ("WrongDestination", Value::None),
        ("WrongMemberOperand", Value::None),
        ("InvalidSetMemberOperand", Value::Int(7)),
    ] {
        let tid = vm.call_global("Fmt13", function, &[]).unwrap();
        assert!(vm.run_until_idle(1000));
        assert_eq!(vm.thread_result(tid), Some(expected));
    }
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("struct member count type does not match destination")));
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("struct_get member operand must be a nonempty string")));
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("struct_set member operand must be a nonempty string")));
}

#[test]
fn fmt_13_struct_state_round_trips_and_skyrim_rejects_fo4_opcodes() {
    let function = FunctionBuilder::new("Fields", "Int")
        .local("count", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructSet, vec![ident("entry"), s("count"), i(12)])
        .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
        .ins(Return, vec![ident("count")]);
    let script = fo4_script(vec![function]);
    let mut vm = Vm::new();
    vm.load(script.clone());
    let actor = vm.create_object("actor");
    vm.attach(actor, "Fmt13").unwrap();
    let tids = vm.send_event(actor, "Fields", &[]);
    assert!(vm.run_until_idle(1000));
    assert_eq!(vm.thread_result(tids[0]), Some(Value::Int(12)));
    let Value::Struct(structure) = vm.get_var(actor, "Fmt13", "entry").unwrap() else {
        panic!("struct variable is saved as a struct value");
    };
    assert_eq!(
        structure.fields.borrow().get("count"),
        Some(&Value::Int(12))
    );
    let saved = vm.save_state().unwrap();
    let mut restored = Vm::new();
    restored.load(script);
    restored.load_state(&saved).unwrap();
    let Value::Struct(structure) = restored.get_var(actor, "Fmt13", "entry").unwrap() else {
        panic!("restored variable is a struct value");
    };
    assert_eq!(
        structure.fields.borrow().get("count"),
        Some(&Value::Int(12))
    );

    for (game_id, minor, name) in [(1, 2, "Skyrim"), (2, 8, "Fallout38")] {
        let check = FunctionBuilder::new("Check", "Bool")
            .param("subject", "Object")
            .local("matched", "Bool")
            .ins(Is, vec![ident("matched"), ident("subject"), ident("Actor")])
            .ins(Return, vec![ident("matched")]);
        let fields = FunctionBuilder::new("Fields", "Int")
            .local("entry", "Entry")
            .local("count", "Int")
            .ins(StructCreate, vec![ident("entry")])
            .ins(StructSet, vec![ident("entry"), s("count"), i(12)])
            .ins(StructGet, vec![ident("count"), ident("entry"), s("count")])
            .ins(Return, vec![ident("count")]);
        let mut script = fo4_script(vec![check, fields]);
        script.objects[0].name = name.to_string();
        script.header.game_id = game_id;
        script.header.minor = minor;
        let mut old_vm = Vm::new();
        old_vm.load(script);
        let actor = old_vm.create_object_of("actor", "Actor");
        let tid = old_vm
            .call_global(name, "Check", &[Value::Object(actor)])
            .unwrap();
        assert!(old_vm.run_until_idle(1000));
        assert_eq!(old_vm.thread_result(tid), Some(Value::Bool(false)));
        let tid = old_vm.call_global(name, "Fields", &[]).unwrap();
        assert!(old_vm.run_until_idle(1000));
        assert_eq!(old_vm.thread_result(tid), Some(Value::None));
        assert!(old_vm
            .log
            .entries
            .iter()
            .any(|entry| entry.message.contains("requires a Fallout 4")));
    }
}
