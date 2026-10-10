// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::{Assign, PropGet, PropSet, Return};
use soe_papyrus::{ObjectId, Value, Vm};

fn caller() -> Vec<u8> {
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
                .build(),
        )
        .bytes()
}

fn scripts() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let base = ScriptBuilder::new("NativeBase", "")
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
        .bytes();
    let middle = ScriptBuilder::new("NativeMiddle", "NativeBase").bytes();
    let leaf = ScriptBuilder::new("NativeLeaf", "NativeMiddle").bytes();
    (base, middle, leaf)
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
fn scr_75_multilevel_native_class_finds_parent_accessor_and_keeps_one_attachment() {
    let mut vm = Vm::new();
    let (base, middle, leaf) = scripts();
    vm.load_bytes(&base).unwrap();
    vm.load_bytes(&middle).unwrap();
    vm.load_bytes(&leaf).unwrap();
    vm.load_bytes(&caller()).unwrap();
    let target = vm.create_object_of("global", "NativeLeaf");

    write(&mut vm, target, 8.25);

    assert_eq!(read(&mut vm, target), Value::Float(8.25));
    assert_eq!(vm.scripts_on(target), ["NativeLeaf"]);
}

#[test]
fn scr_75_repeated_access_keeps_values_separate_per_native_form() {
    let mut vm = Vm::new();
    let (base, middle, leaf) = scripts();
    vm.load_bytes(&base).unwrap();
    vm.load_bytes(&middle).unwrap();
    vm.load_bytes(&leaf).unwrap();
    vm.load_bytes(&caller()).unwrap();
    let first = vm.create_object_of("first", "NativeLeaf");
    let second = vm.create_object_of("second", "NativeLeaf");

    write(&mut vm, first, 2.0);
    write(&mut vm, first, 3.5);
    write(&mut vm, second, -4.0);

    assert_eq!(read(&mut vm, first), Value::Float(3.5));
    assert_eq!(read(&mut vm, second), Value::Float(-4.0));
    assert_eq!(vm.scripts_on(first), ["NativeLeaf"]);
    assert_eq!(vm.scripts_on(second), ["NativeLeaf"]);
}

#[test]
fn scr_75_parent_accessor_storage_survives_vm_save_and_restore() {
    let (base, middle, leaf) = scripts();
    let caller = caller();
    let mut vm = Vm::new();
    for script in [&base, &middle, &leaf, &caller] {
        vm.load_bytes(script).unwrap();
    }
    let target = vm.create_object_of("global", "NativeLeaf");
    write(&mut vm, target, 42.0);
    let saved = vm.save_state().unwrap();

    let mut restored = Vm::new();
    for script in [&base, &middle, &leaf, &caller] {
        restored.load_bytes(script).unwrap();
    }
    restored.load_state(&saved).unwrap();

    assert_eq!(read(&mut restored, target), Value::Float(42.0));
    assert_eq!(restored.scripts_on(target), ["NativeLeaf"]);
}
