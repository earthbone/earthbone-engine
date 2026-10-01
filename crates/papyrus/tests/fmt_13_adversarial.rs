// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::{Opcode::*, StructDefinition, StructMember};
use soe_papyrus::{Value, Vm};

fn script(functions: Vec<FunctionBuilder>) -> soe_papyrus::pex::Script {
    let mut builder = ScriptBuilder::new("Fmt13Adversarial", "")
        .variable("entry", "Entry", none())
        .variable("child", "Child", none())
        .variable("other", "Other", none());
    for function in functions {
        builder = builder.function(function.build());
    }
    let mut script = builder.build();
    script.header.game_id = 2;
    script.header.major = 3;
    script.header.minor = 9;
    script.objects[0].structs = vec![
        StructDefinition {
            name: "Entry".into(),
            members: vec![StructMember {
                name: "child".into(),
                type_name: "Child".into(),
                user_flags: 0,
                default: none(),
                is_const: false,
                docstring: String::new(),
            }],
        },
        StructDefinition {
            name: "Child".into(),
            members: Vec::new(),
        },
        StructDefinition {
            name: "Other".into(),
            members: Vec::new(),
        },
    ];
    script
}

fn result(vm: &mut Vm, function: &str) -> Option<Value> {
    let tid = vm.call_global("Fmt13Adversarial", function, &[]).unwrap();
    assert!(vm.run_until_idle(1000));
    vm.thread_result(tid)
}

#[test]
fn fmt_13_struct_set_accepts_value_of_declared_child_struct_type() {
    let function = FunctionBuilder::new("SetChild", "Child")
        .local("entry", "Entry")
        .local("child", "Child")
        .local("readback", "Child")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructCreate, vec![ident("child")])
        .ins(StructSet, vec![ident("entry"), s("child"), ident("child")])
        .ins(
            StructGet,
            vec![ident("readback"), ident("entry"), s("child")],
        )
        .ins(Return, vec![ident("readback")]);
    let mut vm = Vm::new();
    vm.load(script(vec![function]));
    let value = result(&mut vm, "SetChild");
    assert!(
        matches!(value, Some(Value::Struct(_))),
        "expected Child struct, got {value:?}; log: {:?}",
        vm.log.entries
    );
    assert!(!vm.log.entries.iter().any(|entry| entry
        .message
        .contains("does not match struct member child type Child")));
}

#[test]
fn fmt_13_struct_set_rejects_value_of_different_struct_type() {
    let function = FunctionBuilder::new("RejectOther", "Child")
        .local("entry", "Entry")
        .local("other", "Other")
        .local("readback", "Child")
        .ins(StructCreate, vec![ident("entry")])
        .ins(StructCreate, vec![ident("other")])
        .ins(StructSet, vec![ident("entry"), s("child"), ident("other")])
        .ins(
            StructGet,
            vec![ident("readback"), ident("entry"), s("child")],
        )
        .ins(Return, vec![ident("readback")]);
    let mut vm = Vm::new();
    vm.load(script(vec![function]));
    assert_eq!(result(&mut vm, "RejectOther"), Some(Value::None));
    assert!(vm.log.entries.iter().any(|entry| entry
        .message
        .contains("value of type Struct does not match struct member child type Child")));
}

#[test]
fn fmt_13_struct_get_of_missing_member_returns_none_without_mutating_other_fields() {
    let function = FunctionBuilder::new("Missing", "Int")
        .local("entry", "Entry")
        .local("readback", "Int")
        .ins(StructCreate, vec![ident("entry")])
        .ins(
            StructGet,
            vec![ident("readback"), ident("entry"), s("absent")],
        )
        .ins(Return, vec![ident("readback")]);
    let mut vm = Vm::new();
    vm.load(script(vec![function]));
    assert_eq!(result(&mut vm, "Missing"), Some(Value::None));
    assert!(vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.contains("has no member absent")));
}
