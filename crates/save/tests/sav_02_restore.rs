// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::InstanceId;
use soe_papyrus::{ObjectId, Opcode, Value, Vm};
use soe_save::papyrus::restore::RestoredInstances;
use soe_save::papyrus::Value as SavedValue;
use soe_save::papyrus::{
    ActiveScriptData, Execution, Instruction, MemberDesc, Papyrus, StackFrame, StringRef,
};

fn fixture() -> (Vm, ObjectId, Papyrus) {
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
    vm.load(script);
    let object = vm.create_object("saved");
    vm.attach(object, "Saved").unwrap();

    let strings = [
        "", "Saved", "Outer", "Inner", "result", "input", "self", "Int",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let string =
        |name: &str| StringRef(strings.iter().position(|value| value == name).unwrap() as u32);
    let member = |name: &str| MemberDesc {
        name: string(name),
        type_name: string("Int"),
    };
    let instruction = |opcode, operands| Instruction { opcode, operands };
    let identifier = |name: &str| soe_save::papyrus::Operand::Identifier(string(name));
    let integer = |value| soe_save::papyrus::Operand::Integer(value);
    let owner = || SavedValue::Reference {
        type_name: string("Saved"),
        id: 7,
    };
    let frame = |name: &str,
                 params: Vec<MemberDesc>,
                 locals: Vec<MemberDesc>,
                 instructions: Vec<Instruction>,
                 pointer,
                 variables| StackFrame {
        flags: 0,
        function_type: 1,
        script_name: string("Saved"),
        base_name: string(name),
        event: string(""),
        status: None,
        opcode_version: [3, 2],
        return_type: string("Int"),
        docstring: string(""),
        user_flags: 0,
        function_flags: 0,
        parameters: params,
        locals,
        instructions,
        pointer,
        owner: owner(),
        variables,
    };
    let active = ActiveScriptData {
        id: 10,
        version: [3, 2],
        unknown: SavedValue::Null(0),
        flags: 0,
        unknown_byte: 0,
        extra: None,
        fragment_kind: 0,
        fragment: None,
        frames: vec![
            frame(
                "Outer",
                vec![],
                vec![member("result")],
                vec![
                    instruction(
                        Opcode::CallMethod,
                        vec![
                            soe_save::papyrus::Operand::String(string("Inner")),
                            identifier("self"),
                            identifier("result"),
                            integer(0),
                        ],
                    ),
                    instruction(Opcode::Return, vec![identifier("result")]),
                ],
                1,
                vec![SavedValue::Integer(0)],
            ),
            frame(
                "Inner",
                vec![member("input")],
                vec![member("result")],
                vec![
                    instruction(
                        Opcode::IAdd,
                        vec![identifier("result"), identifier("input"), integer(1)],
                    ),
                    instruction(Opcode::Return, vec![identifier("result")]),
                ],
                1,
                vec![SavedValue::Integer(41), SavedValue::Integer(42)],
            ),
        ],
        trailing: Some(0),
    };
    let papyrus = Papyrus {
        vm_version: 0,
        strings,
        scripts: vec![],
        instances: vec![],
        references: vec![],
        arrays: vec![],
        runtime_id: 0,
        active_scripts: vec![],
        instance_data: vec![],
        reference_data: vec![],
        array_data: vec![],
        execution: Execution {
            active_data: vec![active],
            messages: vec![],
            suspended: [vec![], vec![]],
            unknown: 0,
            extra: None,
            unknown_ids: vec![],
            unbinds: vec![],
            save_file_version: 0,
        },
        data_start: 0,
        execution_start: 0,
        block_len: 0,
        decoded_len: 0,
    };
    (vm, object, papyrus)
}

#[test]
fn sav_02_saved_nested_call_restores_position_locals_and_returns_once() {
    let (mut vm, object, papyrus) = fixture();
    let restored = RestoredInstances {
        instances: BTreeMap::from([(7, InstanceId(0))]),
        arrays: BTreeMap::new(),
        objects: BTreeMap::from([(7, Some(object))]),
    };
    let id = papyrus.restore_execution(&mut vm, &restored).unwrap()[0];
    vm.run_until_idle(100);
    assert_eq!(vm.thread_result(id), Some(Value::Int(42)));
    assert_eq!(vm.run(100), 0);
}

#[test]
fn sav_02_invalid_frame_reference_rejects_without_vm_mutation() {
    let (mut vm, object, mut papyrus) = fixture();
    let restored = RestoredInstances {
        instances: BTreeMap::from([(7, InstanceId(0))]),
        arrays: BTreeMap::new(),
        objects: BTreeMap::from([(7, Some(object))]),
    };
    papyrus.execution.active_data[0].frames[0].pointer = u32::MAX;
    let before = vm.save_state().unwrap();
    assert!(papyrus.restore_execution(&mut vm, &restored).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
}
