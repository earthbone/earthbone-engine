// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::restore::{SavedFrame, SavedThread};
use soe_papyrus::{ObjectId, Opcode, Script, Value, Vm};

fn fixture() -> (Vm, ObjectId, Script) {
    let script = ScriptBuilder::new("Saved", "")
        .function(
            FunctionBuilder::new("Outer", "Int")
                .local("result", "Int")
                .ins(
                    Opcode::CallMethod,
                    vec![s("Inner"), ident("self"), ident("result")],
                )
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .function(
            FunctionBuilder::new("Inner", "Int")
                .param("input", "Int")
                .local("result", "Int")
                .ins(Opcode::IAdd, vec![ident("result"), ident("input"), i(1)])
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .build();
    let mut vm = Vm::new();
    vm.load(script.clone());
    let object = vm.create_object("saved");
    vm.attach(object, "Saved").unwrap();
    (vm, object, script)
}

fn saved_frame(
    object: ObjectId,
    script: &Script,
    name: &str,
    pc: usize,
    locals: Vec<(&str, Value)>,
    return_dest: Option<&str>,
) -> SavedFrame {
    let function = script.object().function_in("", name).unwrap();
    SavedFrame {
        object,
        script: "Saved".into(),
        function: name.into(),
        pc,
        locals: locals
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
        code: function.code.clone(),
        return_dest: return_dest.map(str::to_owned),
    }
}

#[test]
fn sav_02_nested_saved_call_restores_instruction_locals_and_returns_once() {
    let (mut vm, object, script) = fixture();
    let thread = SavedThread {
        frames: vec![
            saved_frame(
                object,
                &script,
                "Outer",
                1,
                vec![("result", Value::Int(0))],
                None,
            ),
            saved_frame(
                object,
                &script,
                "Inner",
                1,
                vec![("input", Value::Int(41)), ("result", Value::Int(42))],
                Some("result"),
            ),
        ],
    };
    let id = vm.restore_threads(&[thread]).unwrap()[0];
    vm.run_until_idle(100);
    assert_eq!(vm.thread_result(id), Some(Value::Int(42)));
    assert_eq!(vm.thread_state(id), Some(soe_papyrus::ThreadState::Done));
}

#[test]
fn sav_02_invalid_saved_frame_references_reject_before_vm_mutation() {
    let (mut vm, object, script) = fixture();
    let before = vm.save_state().unwrap();
    let bad = SavedThread {
        frames: vec![saved_frame(
            object,
            &script,
            "Outer",
            usize::MAX,
            vec![("result", Value::Int(9))],
            None,
        )],
    };
    assert!(vm.restore_threads(&[bad]).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
}
