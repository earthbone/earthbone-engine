// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::*;
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, ObjectId, Value, Vm};

#[test]
fn tst_03_element_kind_defaults_match_declared_types() {
    for (name, kind, default) in [
        ("Int", ElementKind::Int, Value::Int(0)),
        ("Float", ElementKind::Float, Value::Float(0.0)),
        ("Bool", ElementKind::Bool, Value::Bool(false)),
        ("String", ElementKind::Str, Value::string("")),
        ("Actor", ElementKind::Object, Value::None),
    ] {
        assert_eq!(ElementKind::from_type(name), kind);
        assert_eq!(kind.default_value(), default);
    }
    assert_eq!(
        ElementKind::from_array_type("String[]"),
        Some(ElementKind::Str)
    );
    assert_eq!(ElementKind::from_array_type("String"), None);
}

#[test]
fn tst_03_array_identity_and_access_preserve_handles() {
    let shared = ArrayRef::new(ElementKind::Int, 2);
    let same = shared.clone();
    let separate = ArrayRef::new(ElementKind::Int, 2);
    assert_eq!(shared.len(), 2);
    assert_eq!(shared.get(0), Some(Value::Int(0)));
    assert_eq!(shared, same);
    assert_ne!(shared, separate);
    assert_eq!(Value::Array(shared.clone()), Value::Array(same));
    assert_ne!(Value::Array(shared.clone()), Value::Array(separate));
}

#[test]
fn tst_03_scalar_conversions_and_kind_inspection() {
    let object = Value::Object(ObjectId(7));
    assert_eq!(object.as_object(), Some(ObjectId(7)));
    assert!(object.as_array().is_none());
    assert!(Value::None.is_none());
    assert!(!object.is_none());
    assert_eq!(Value::Bool(true).as_int(), 1);
    assert_eq!(Value::Bool(true).as_float(), 1.0);
    assert_eq!(Value::Bool(false).as_float(), 0.0);
    assert_eq!(Value::string("-2.75tail").as_float(), -2.75);
    assert_eq!(Value::string("+1.25rest").as_float(), 1.25);
    assert_eq!(Value::string("1tail").as_float(), 1.0);
    assert_eq!(Value::string("+8tail").as_int(), 8);
    assert_eq!(Value::Int(3).type_name(), "Int");
    assert_eq!(Value::Float(3.0).type_name(), "Float");
}

#[test]
fn tst_03_value_comparisons_cover_kinds_and_boundaries() {
    assert!(Value::string("Alpha").equals(&Value::string("aLPHa")));
    assert!(!Value::string("2").equals(&Value::Int(2)));
    assert!(Value::Int(2).equals(&Value::Float(2.0)));
    assert!(Value::Int(2).equals(&Value::Int(2)));
    assert!(!Value::Int(16_777_217).equals(&Value::Int(16_777_216)));
    assert!(Value::Bool(true).equals(&Value::Bool(true)));
    assert!(!Value::Bool(true).equals(&Value::Bool(false)));
    assert!(Value::Bool(true).equals(&Value::Array(ArrayRef::new(ElementKind::Int, 1))));
    assert!(Value::None.equals(&Value::None));
    let array = Value::Array(ArrayRef::new(ElementKind::Int, 0));
    assert!(!Value::None.equals(&array));
    assert!(!Value::Object(ObjectId(1)).equals(&Value::Object(ObjectId(2))));
    assert!(!Value::None.equals(&Value::Object(ObjectId(1))));
    assert!(Value::Bool(true).equals(&Value::Int(1)));
    assert!(!Value::Bool(false).equals(&Value::Int(1)));

    assert!(Value::Int(2).less_than(&Value::Int(3)));
    assert!(Value::Int(16_777_216).less_than(&Value::Int(16_777_217)));
    assert!(!Value::Int(3).less_than(&Value::Int(2)));
    assert!(!Value::Int(2).less_than(&Value::Int(2)));
    assert!(Value::string("Alpha").less_than(&Value::string("beta")));
    assert!(!Value::string("beta").less_than(&Value::string("Alpha")));
    assert!(!Value::string("Alpha").less_than(&Value::string("alpha")));
    assert!(!Value::Float(2.0).less_than(&Value::Float(2.0)));
    assert_eq!(Value::string(".5rest").as_float(), 0.5);
    assert_eq!(Value::string("no number").as_float(), 0.0);
    assert_eq!(Value::string("1-2").as_int(), 1);
    assert_eq!(Value::string("--1").as_int(), 0);
    assert_eq!(Value::string("1..2").as_float(), 1.0);
    assert_eq!(Value::string("--1.0").as_float(), 0.0);
}

#[test]
fn tst_03_vm_dispatches_static_calls_and_executes_integer_arithmetic() {
    let add = FunctionBuilder::new("Add", "Int")
        .param("left", "Int")
        .param("right", "Int")
        .local("result", "Int")
        .ins(IAdd, vec![ident("result"), ident("left"), ident("right")])
        .ins(Return, vec![ident("result")])
        .build();
    let caller = FunctionBuilder::new("Run", "Int")
        .local("result", "Int")
        .ins(
            CallStatic,
            vec![s("Math"), s("Add"), ident("result"), i(19), i(23)],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Math", "")
        .function(add)
        .function(caller)
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    let tid = vm.call_global("Math", "Run", &[]).unwrap();
    assert!(vm.run_until_idle(100));
    assert_eq!(vm.thread_result(tid), Some(Value::Int(42)));
}
