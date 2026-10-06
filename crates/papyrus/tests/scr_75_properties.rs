// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::{CallMethod, CallStatic, PropGet, PropSet, Return};
use soe_papyrus::{NativeReturn, ObjectId, ThreadState, Value, Vm, VmError};

fn accessors() -> Vec<u8> {
    let getter = FunctionBuilder::new("get", "Float")
        .local("result", "Float")
        .ins(
            CallMethod,
            vec![ident("GetValue"), ident("self"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let setter = FunctionBuilder::new("set", "None")
        .param("value", "Float")
        .local("ignored", "None")
        .ins(
            CallMethod,
            vec![
                ident("SetValue"),
                ident("self"),
                ident("ignored"),
                ident("value"),
            ],
        )
        .build();
    ScriptBuilder::new("NativeBase", "")
        .property("Value", "Float", Some(getter.clone()), Some(setter.clone()))
        .property("ReadOnly", "Float", Some(getter), None)
        .property("WriteOnly", "Float", None, Some(setter))
        .function(FunctionBuilder::new("GetValue", "Float").native().build())
        .function(
            FunctionBuilder::new("SetValue", "None")
                .native()
                .param("value", "Float")
                .build(),
        )
        .bytes()
}

fn caller() -> Vec<u8> {
    ScriptBuilder::new("Caller", "")
        .function(
            FunctionBuilder::new("Read", "Float")
                .global()
                .param("target", "NativeBase")
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
                .param("target", "NativeBase")
                .param("value", "Float")
                .ins(
                    PropSet,
                    vec![ident("VaLuE"), ident("target"), ident("value")],
                )
                .build(),
        )
        .bytes()
}

fn fixture() -> Vm {
    let mut vm = Vm::new();
    vm.load_bytes(&accessors()).unwrap();
    vm.load_bytes(&caller()).unwrap();
    vm.natives.register_method(
        "NativeBase",
        "GetValue",
        Box::new(|call| NativeReturn::Value(Value::Float(call.self_object.unwrap().0 as f32))),
    );
    vm.natives.register_method(
        "NativeBase",
        "SetValue",
        Box::new(|call| {
            call.log.push(
                call.time,
                format!("{}={}", call.self_object.unwrap().0, call.arg(0).as_float()),
            );
            NativeReturn::Value(Value::None)
        }),
    );
    vm
}

fn read(vm: &mut Vm, target: ObjectId) -> Value {
    let tid = vm
        .call_global("Caller", "Read", &[Value::Object(target)])
        .unwrap();
    assert!(vm.run_until_idle(10_000));
    vm.thread_result(tid).unwrap()
}

fn write(vm: &mut Vm, target: ObjectId, value: f32) {
    vm.call_global(
        "Caller",
        "Write",
        &[Value::Object(target), Value::Float(value)],
    )
    .unwrap();
    assert!(vm.run_until_idle(10_000));
}

#[test]
fn scr_75_native_properties_call_accessors_with_target_and_case_insensitive_names() {
    let mut vm = fixture();
    let first = vm.create_object_of("first", "nAtIvEbAsE");
    let second = vm.create_object_of("second", "NativeBase");
    assert!(vm.scripts_on(first).is_empty());
    assert_eq!(read(&mut vm, first), Value::Float(first.0 as f32));
    assert_eq!(read(&mut vm, second), Value::Float(second.0 as f32));
    write(&mut vm, first, 12.5);
    write(&mut vm, second, -2.0);
    assert_eq!(vm.scripts_on(first), ["NativeBase"]);
    assert_eq!(
        vm.log.messages(),
        [format!("{}=12.5", first.0), format!("{}=-2", second.0)]
    );
}

#[test]
fn scr_75_native_properties_resolve_loaded_and_registry_parents() {
    for loaded in [true, false] {
        let mut vm = fixture();
        if loaded {
            vm.load_bytes(&ScriptBuilder::new("Derived", "NativeBase").bytes())
                .unwrap();
        } else {
            vm.natives.declare_parent("Derived", "NativeBase");
        }
        let target = vm.create_object_of("target", "dErIvEd");
        write(&mut vm, target, 3.0);
        assert_eq!(read(&mut vm, target), Value::Float(target.0 as f32));
        assert_eq!(vm.scripts_on(target).len(), 1);
        assert_eq!(vm.log.messages(), [format!("{}=3", target.0)]);
    }
}

#[test]
fn scr_75_attached_script_inherits_accessors_without_duplicate_instances() {
    let mut vm = fixture();
    vm.load_bytes(&ScriptBuilder::new("Derived", "NativeBase").bytes())
        .unwrap();
    let target = vm.create_object_of("target", "NativeBase");
    vm.attach(target, "Derived").unwrap();
    write(&mut vm, target, 4.0);
    assert_eq!(read(&mut vm, target), Value::Float(target.0 as f32));
    assert_eq!(vm.scripts_on(target), ["Derived"]);
}

#[test]
fn scr_75_host_accessors_resolve_native_form_parent_scripts() {
    let mut vm = fixture();
    vm.load_bytes(&ScriptBuilder::new("Derived", "NativeBase").bytes())
        .unwrap();
    let target = vm.create_object_of("target", "Derived");
    vm.set_property(target, "nAtIvEbAsE", "vAlUe", Value::Float(6.0))
        .unwrap();
    assert_eq!(
        vm.get_property(target, "NativeBase", "VALUE"),
        Some(Value::Float(target.0 as f32))
    );
    assert_eq!(vm.scripts_on(target), ["Derived"]);
    assert_eq!(vm.log.messages(), [format!("{}=6", target.0)]);
}

#[test]
fn scr_75_access_permissions_and_missing_properties_report_errors() {
    let mut vm = fixture();
    let target = vm.create_object_of("target", "NativeBase");
    let probe = ScriptBuilder::new("Errors", "")
        .function(
            FunctionBuilder::new("Probe", "None")
                .global()
                .param("target", "NativeBase")
                .local("result", "Float")
                .ins(
                    PropGet,
                    vec![ident("Missing"), ident("target"), ident("result")],
                )
                .ins(PropSet, vec![ident("Missing"), ident("target"), f(7.0)])
                .ins(
                    PropGet,
                    vec![ident("writeonly"), ident("target"), ident("result")],
                )
                .ins(PropSet, vec![ident("readonly"), ident("target"), f(7.0)])
                .build(),
        )
        .bytes();
    vm.load_bytes(&probe).unwrap();
    vm.call_global("Errors", "Probe", &[Value::Object(target)])
        .unwrap();
    assert!(vm.run_until_idle(10_000));
    let messages = vm.log.messages();
    assert_eq!(messages.len(), 4);
    assert!(messages[0].contains("has no property Missing"));
    assert!(messages[1].contains("has no property Missing"));
    assert!(messages[2].contains("property writeonly is write-only"));
    assert!(messages[3].contains("property readonly is read-only"));
    assert!(matches!(
        vm.set_property(target, "NativeBase", "ReadOnly", Value::Float(7.0)),
        Err(VmError::ReadOnlyProperty { .. })
    ));
    assert_eq!(vm.get_property(target, "NativeBase", "WriteOnly"), None);
}

#[test]
fn scr_75_inherited_accessor_suspends_and_survives_vm_restore() {
    let mut vm = Vm::new();
    let base = ScriptBuilder::new("NativeBase", "")
        .variable("stored", "Float", f(2.0))
        .property(
            "Value",
            "Float",
            Some(
                FunctionBuilder::new("get", "Float")
                    .local("ignored", "None")
                    .ins(
                        CallStatic,
                        vec![ident("Utility"), ident("Wait"), ident("ignored"), f(1.0)],
                    )
                    .ins(Return, vec![ident("stored")])
                    .build(),
            ),
            Some(
                FunctionBuilder::new("set", "None")
                    .param("value", "Float")
                    .ins(
                        soe_papyrus::pex::Opcode::Assign,
                        vec![ident("stored"), ident("value")],
                    )
                    .ins(Return, vec![none()])
                    .build(),
            ),
        )
        .bytes();
    vm.load_bytes(&base).unwrap();
    vm.load_bytes(&caller()).unwrap();
    let derived = ScriptBuilder::new("Derived", "NativeBase").bytes();
    vm.load_bytes(&derived).unwrap();
    let target = vm.create_object_of("target", "Derived");
    write(&mut vm, target, 9.0);
    let tid = vm
        .call_global("Caller", "Read", &[Value::Object(target)])
        .unwrap();
    vm.run(10_000);
    assert!(matches!(
        vm.thread_state(tid),
        Some(ThreadState::Sleeping(_))
    ));
    let saved = vm.save_state().unwrap();
    let mut restored = Vm::new();
    restored.load_bytes(&base).unwrap();
    restored.load_bytes(&caller()).unwrap();
    restored.load_bytes(&derived).unwrap();
    restored.load_state(&saved).unwrap();
    restored.advance(1.0);
    assert!(restored.run_until_idle(10_000));
    assert_eq!(restored.thread_result(tid), Some(Value::Float(9.0)));
    assert_eq!(restored.scripts_on(target), ["Derived"]);
    assert!(restored.log.messages().is_empty());
}
