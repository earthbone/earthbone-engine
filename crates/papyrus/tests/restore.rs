// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Restored private variables retain their declaring script's storage.

use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::restore::{InstanceState, SavedVariable};
use soe_papyrus::{ObjectId, Opcode, Value, Vm};

fn fixture() -> (Vm, ObjectId, InstanceState) {
    let mut vm = Vm::new();
    for (name, parent, method) in [
        ("Parent", "", "ParentIncrement"),
        ("Child", "Parent", "ChildIncrement"),
    ] {
        vm.load(
            ScriptBuilder::new(name, parent)
                .variable("count", "Int", i(0))
                .auto_property("Stored", "Int", i(0))
                .function(
                    FunctionBuilder::new(method, "Int")
                        .ins(Opcode::IAdd, vec![ident("count"), ident("count"), i(1)])
                        .ins(Opcode::Return, vec![ident("count")])
                        .build(),
                )
                .build(),
        );
    }
    let object = vm.create_object("saved");
    let variable = |owner: &str, name: &str, value| SavedVariable {
        declaring_script: owner.into(),
        name: name.into(),
        type_name: "Int".into(),
        value: Value::Int(value),
    };
    let state = InstanceState {
        object,
        script: "Child".into(),
        state: "NoBody".into(),
        variables: vec![
            variable("Parent", "count", 10),
            variable("Child", "count", 100),
            variable("Parent", "::Stored_var", 20),
            variable("Child", "::Stored_var", 200),
        ],
    };
    (vm, object, state)
}

#[test]
fn same_named_parent_and_child_variables_execute_and_persist_independently() {
    let (mut vm, object, state) = fixture();
    vm.restore_instances(&[state]).unwrap();
    assert_eq!(vm.state_of(object, "Child").as_deref(), Some("NoBody"));
    for (function, expected) in [("ParentIncrement", 11), ("ChildIncrement", 101)] {
        let thread = vm.call(object, function, &[]).unwrap();
        vm.run_until_idle(100);
        assert_eq!(vm.thread_result(thread), Some(Value::Int(expected)));
    }
    let bytes = vm.save_state().unwrap();
    let (mut fresh, _, _) = fixture();
    fresh.load_state(&bytes).unwrap();
    assert_eq!(
        fresh.get_var(object, "Parent", "count"),
        Some(Value::Int(11))
    );
    assert_eq!(
        fresh.get_var(object, "Child", "count"),
        Some(Value::Int(101))
    );
    assert_eq!(
        fresh.get_var(object, "Parent", "::Stored_var"),
        Some(Value::Int(20))
    );
    assert_eq!(
        fresh.get_property(object, "Child", "Stored"),
        Some(Value::Int(200))
    );
    fresh
        .set_property(object, "Child", "Stored", Value::Int(300))
        .unwrap();
    assert_eq!(
        fresh.get_var(object, "Parent", "::Stored_var"),
        Some(Value::Int(20))
    );
}

#[test]
fn duplicate_declarations_or_missing_owner_do_not_replace_valid_instances() {
    let (mut vm, object, state) = fixture();
    vm.attach(object, "Child").unwrap();
    let before = vm.save_state().unwrap();
    let mut invalid = state.clone();
    invalid.variables.push(state.variables[0].clone());
    assert!(vm.restore_instances(&[invalid]).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
    let mut invalid = state;
    invalid.variables[0].declaring_script = "Unrelated".into();
    assert!(vm.restore_instances(&[invalid]).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
}

#[test]
fn restoration_refuses_to_invalidate_existing_thread_frames() {
    let (mut vm, object, state) = fixture();
    vm.attach(object, "Child").unwrap();
    let thread = vm.call(object, "ChildIncrement", &[]).unwrap();
    let before = vm.save_state().unwrap();
    assert!(vm.restore_instances(&[state]).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
    vm.run_until_idle(100);
    assert_eq!(vm.thread_result(thread), Some(Value::Int(1)));
}
