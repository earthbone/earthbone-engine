// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::{Assign, CallStatic, PropGet, PropSet, Return};
use soe_papyrus::{ObjectId, ThreadState, Value, Vm};

fn base_script() -> Vec<u8> {
    ScriptBuilder::new("NativeBase", "")
        .variable("stored", "Float", f(1.0))
        .property(
            "Value",
            "Float",
            Some(
                FunctionBuilder::new("get", "Float")
                    .local("result", "Float")
                    .ins(Assign, vec![ident("result"), ident("stored")])
                    .ins(Return, vec![ident("result")])
                    .build(),
            ),
            Some(
                FunctionBuilder::new("set", "None")
                    .param("value", "Float")
                    .ins(Assign, vec![ident("stored"), ident("value")])
                    .build(),
            ),
        )
        .property(
            "ReadOnly",
            "Float",
            Some(
                FunctionBuilder::new("get", "Float")
                    .ins(Return, vec![f(9.0)])
                    .build(),
            ),
            None,
        )
        .property(
            "WriteOnly",
            "Float",
            None,
            Some(
                FunctionBuilder::new("set", "None")
                    .param("value", "Float")
                    .build(),
            ),
        )
        .bytes()
}

fn caller_script() -> Vec<u8> {
    ScriptBuilder::new("Caller", "")
        .function(
            FunctionBuilder::new("Read", "Float")
                .global()
                .param("target", "NativeLeaf")
                .local("result", "Float")
                .ins(
                    PropGet,
                    vec![ident("vAlUe"), ident("target"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Write", "None")
                .global()
                .param("target", "NativeLeaf")
                .param("value", "Float")
                .ins(
                    PropSet,
                    vec![ident("VALUE"), ident("target"), ident("value")],
                )
                .ins(Return, vec![])
                .build(),
        )
        .function(
            FunctionBuilder::new("WaitRead", "Float")
                .global()
                .param("target", "NativeLeaf")
                .local("ignored", "None")
                .local("result", "Float")
                .ins(
                    CallStatic,
                    vec![ident("Utility"), ident("Wait"), ident("ignored"), f(1.0)],
                )
                .ins(
                    PropGet,
                    vec![ident("Value"), ident("target"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes()
}

fn fixture() -> (Vm, ObjectId) {
    let mut vm = Vm::new();
    vm.load_bytes(&base_script()).unwrap();
    vm.load_bytes(&ScriptBuilder::new("NativeMiddle", "NativeBase").bytes())
        .unwrap();
    vm.load_bytes(&ScriptBuilder::new("NativeLeaf", "NativeMiddle").bytes())
        .unwrap();
    vm.load_bytes(&caller_script()).unwrap();
    let target = vm.create_object_of("native global", "NativeLeaf");
    (vm, target)
}

fn read(vm: &mut Vm, target: ObjectId) -> Value {
    let thread = vm
        .call_global("Caller", "Read", &[Value::Object(target)])
        .unwrap();
    assert!(vm.run_until_idle(10_000));
    vm.thread_result(thread).unwrap()
}

fn write(vm: &mut Vm, target: ObjectId, value: f32) {
    let thread = vm
        .call_global(
            "Caller",
            "Write",
            &[Value::Object(target), Value::Float(value)],
        )
        .unwrap();
    assert!(vm.run_until_idle(10_000));
    assert_eq!(vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_75_parent_getter_and_setter_run_on_deep_native_subclass() {
    let (mut vm, target) = fixture();
    write(&mut vm, target, -12.75);
    assert_eq!(read(&mut vm, target), Value::Float(-12.75));
    assert_eq!(vm.scripts_on(target), ["NativeLeaf"]);
    assert!(vm.log.messages().is_empty());
}

#[test]
fn scr_75_each_native_object_keeps_its_own_accessor_backing_value() {
    let (mut vm, first) = fixture();
    let second = vm.create_object_of("second native global", "NativeLeaf");
    write(&mut vm, first, 2.5);
    write(&mut vm, first, 3.5);
    write(&mut vm, second, -4.25);
    assert_eq!(read(&mut vm, first), Value::Float(3.5));
    assert_eq!(read(&mut vm, second), Value::Float(-4.25));
    assert_eq!(vm.scripts_on(first), ["NativeLeaf"]);
    assert_eq!(vm.scripts_on(second), ["NativeLeaf"]);
}

#[test]
fn scr_75_host_property_api_resolves_parent_accessor_from_native_class() {
    let (mut vm, target) = fixture();
    vm.set_property(target, "nAtIvEbAsE", "vAlUe", Value::Float(6.25))
        .unwrap();
    assert_eq!(
        vm.get_property(target, "NATIVEBASE", "VALUE"),
        Some(Value::Float(6.25))
    );
    assert_eq!(vm.scripts_on(target), ["NativeLeaf"]);
}

#[test]
fn scr_75_unknown_and_one_sided_properties_do_not_call_wrong_accessors() {
    let (mut vm, target) = fixture();
    let probe = ScriptBuilder::new("Probe", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "NativeLeaf")
                .local("result", "Float")
                .ins(
                    PropGet,
                    vec![ident("Missing"), ident("target"), ident("result")],
                )
                .ins(
                    PropGet,
                    vec![ident("WriteOnly"), ident("target"), ident("result")],
                )
                .ins(PropSet, vec![ident("ReadOnly"), ident("target"), f(10.0)])
                .build(),
        )
        .bytes();
    vm.load_bytes(&probe).unwrap();
    let thread = vm
        .call_global("Probe", "Run", &[Value::Object(target)])
        .unwrap();
    assert!(vm.run_until_idle(10_000));
    assert_eq!(vm.thread_result(thread), Some(Value::None));
    let messages = vm.log.messages();
    assert_eq!(messages.len(), 3);
    assert!(messages[0].contains("has no property Missing"));
    assert!(messages[1].contains("WriteOnly is write-only"));
    assert!(messages[2].contains("ReadOnly is read-only"));
    assert!(vm
        .log
        .messages()
        .iter()
        .all(|message| !message.contains("stored")));
}

#[test]
fn scr_75_suspended_inherited_getter_survives_engine_save_and_load() {
    let (mut vm, target) = fixture();
    write(&mut vm, target, 21.5);
    let thread = vm
        .call_global("Caller", "WaitRead", &[Value::Object(target)])
        .unwrap();
    vm.run(10_000);
    assert!(matches!(
        vm.thread_state(thread),
        Some(ThreadState::Sleeping(_))
    ));
    let state = vm.save_state().unwrap();

    let mut restored = Vm::new();
    restored.load_bytes(&base_script()).unwrap();
    restored
        .load_bytes(&ScriptBuilder::new("NativeMiddle", "NativeBase").bytes())
        .unwrap();
    restored
        .load_bytes(&ScriptBuilder::new("NativeLeaf", "NativeMiddle").bytes())
        .unwrap();
    restored.load_bytes(&caller_script()).unwrap();
    restored.load_state(&state).unwrap();
    restored.advance(1.0);
    assert!(restored.run_until_idle(10_000));
    assert_eq!(restored.thread_result(thread), Some(Value::Float(21.5)));
    assert_eq!(restored.scripts_on(target), ["NativeLeaf"]);
}
