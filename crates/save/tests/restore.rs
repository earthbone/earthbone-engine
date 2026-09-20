// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Saved variable restoration through the public save and VM APIs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Value as RuntimeValue, Vm};
use soe_save::papyrus::{
    ArrayData, ArrayInfo, MemberDesc, Papyrus, Reference, ScriptDef, ScriptInstance, StringRef,
    Value, ValueType, VariableData,
};
use soe_save::RefId;

fn fixture() -> (Papyrus, Vm, BTreeMap<u64, Option<soe_papyrus::ObjectId>>) {
    let strings = [
        "", "Parent", "Child", "count", "Int", "shared", "Int[]", "peer", "Child", "Saved", "New",
    ]
    .map(str::to_owned)
    .to_vec();
    let member = |name, type_name| MemberDesc {
        name: StringRef(name),
        type_name: StringRef(type_name),
    };
    let body = |id, count, peer| VariableData {
        id,
        flags: 0,
        state_or_type: StringRef(9),
        unknown: 0,
        extra: None,
        variables: vec![
            Value::Integer(count),
            Value::Array {
                element_type: ValueType::Integer,
                type_name: None,
                id: 100,
            },
            Value::Reference {
                type_name: StringRef(8),
                id: peer,
            },
        ],
    };
    let saved = Papyrus {
        vm_version: 2,
        strings,
        scripts: vec![
            ScriptDef {
                name: StringRef(1),
                parent: StringRef(0),
                members: vec![member(3, 4), member(5, 6)],
            },
            ScriptDef {
                name: StringRef(2),
                parent: StringRef(1),
                members: vec![member(7, 8)],
            },
        ],
        instances: [1, 2]
            .map(|id| ScriptInstance {
                id,
                script: StringRef(2),
                unknown_2bits: 0,
                unknown: u16::MAX,
                form: RefId::from_bytes([0, 0, 0]),
                unknown_byte: 0,
            })
            .to_vec(),
        references: vec![Reference {
            id: 3,
            script: StringRef(2),
        }],
        arrays: vec![ArrayInfo {
            id: 100,
            element_type: ValueType::Integer,
            type_name: None,
            length: 2,
        }],
        runtime_id: 0,
        active_scripts: vec![],
        instance_data: vec![body(2, 20, 1), body(1, 10, 2)],
        reference_data: vec![body(3, 30, 1)],
        array_data: vec![ArrayData {
            id: 100,
            variables: vec![Value::Integer(5), Value::Integer(9)],
        }],
        execution: soe_save::papyrus::Execution {
            active_data: vec![],
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
    let parent = ScriptBuilder::new("Parent", "")
        .variable("count", "Int", i(-1))
        .variable("shared", "Int[]", none())
        .build();
    let child = ScriptBuilder::new("Child", "Parent")
        .variable("peer", "Child", none())
        .auto_state("New")
        .function_in(
            "New",
            FunctionBuilder::new("Compute", "Int")
                .ins(Opcode::Return, vec![i(-99)])
                .build(),
        )
        .function_in(
            "Saved",
            FunctionBuilder::new("Compute", "Int")
                .local("element", "Int")
                .ins(
                    Opcode::ArrayGetElement,
                    vec![ident("element"), ident("shared"), i(0)],
                )
                .ins(
                    Opcode::IAdd,
                    vec![ident("count"), ident("count"), ident("element")],
                )
                .ins(Opcode::Return, vec![ident("count")])
                .build(),
        )
        .function_in(
            "Saved",
            FunctionBuilder::new("OnBeginState", "None")
                .ins(Opcode::Assign, vec![ident("count"), i(999)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(Opcode::Assign, vec![ident("count"), i(888)])
                .build(),
        )
        .build();
    let mut vm = Vm::new();
    vm.load(parent);
    vm.load(child);
    let mapping = (1..=3)
        .map(|id| (id, Some(vm.create_object_of("saved object", "Child"))))
        .collect();
    (saved, vm, mapping)
}

#[test]
fn restored_state_dispatches_with_inherited_values_without_init_events() {
    let (saved, mut vm, mapping) = fixture();
    let imported = saved.restore_instances(&mut vm, &mapping).unwrap();
    assert_eq!(imported.instances.len(), 3);
    assert_eq!(vm.run(100), 0);
    for (id, expected) in [(1, 15), (2, 25), (3, 35)] {
        let object = mapping[&id].unwrap();
        assert_eq!(vm.state_of(object, "Child").as_deref(), Some("Saved"));
        let thread = vm.call(object, "Compute", &[]).unwrap();
        assert!(vm.run_until_idle(100));
        assert_eq!(vm.thread_result(thread), Some(RuntimeValue::Int(expected)));
    }
}

#[test]
fn restored_arrays_and_objects_keep_reference_identity_through_vm_save_load() {
    let (saved, mut vm, mapping) = fixture();
    let imported = saved.restore_instances(&mut vm, &mapping).unwrap();
    let object = mapping[&1].unwrap();
    let peer = mapping[&2].unwrap();
    assert_eq!(
        vm.get_var(object, "Child", "peer"),
        Some(RuntimeValue::Object(peer))
    );
    let array = vm.get_var(object, "Child", "shared").unwrap();
    let array = array.as_array().unwrap();
    assert!(array.same(&imported.arrays[&100]));
    imported.arrays[&100].items.borrow_mut()[0] = RuntimeValue::Int(77);
    let state = vm.save_state().unwrap();
    let (_, mut fresh, _) = fixture();
    fresh.load_state(&state).unwrap();
    let first = fresh.get_var(object, "Child", "shared").unwrap();
    let second = fresh.get_var(peer, "Child", "shared").unwrap();
    assert!(first.as_array().unwrap().same(second.as_array().unwrap()));
    assert_eq!(
        first.as_array().unwrap().get(0),
        Some(RuntimeValue::Int(77))
    );
}

#[test]
fn missing_mapping_or_late_schema_error_leaves_live_instances_unchanged() {
    let (mut saved, mut vm, mut mapping) = fixture();
    vm.attach(mapping[&1].unwrap(), "Child").unwrap();
    let before = vm.save_state().unwrap();
    mapping.remove(&2);
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
    let (original, _, correct) = fixture();
    mapping = correct;
    saved.reference_data[0].variables.pop();
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
    saved = original;
    saved.strings[8] = "Float".into();
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    assert_eq!(vm.save_state().unwrap(), before);
}

#[test]
fn cyclic_layouts_duplicate_targets_and_wrong_array_types_are_rejected() {
    let (mut saved, mut vm, mapping) = fixture();
    saved.scripts[0].parent = StringRef(2);
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    let (saved, mut vm, mut mapping) = fixture();
    mapping.insert(2, mapping[&1]);
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    let (mut saved, mut vm, mapping) = fixture();
    saved.array_data[0].variables[0] = Value::Boolean(1);
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
}

#[test]
fn unresolved_references_require_an_explicit_null_mapping() {
    let (mut saved, mut vm, mut mapping) = fixture();
    saved.instance_data[1].variables[2] = Value::Reference {
        type_name: StringRef(8),
        id: 999,
    };
    assert!(saved.restore_instances(&mut vm, &mapping).is_err());
    mapping.insert(999, None);
    saved.restore_instances(&mut vm, &mapping).unwrap();
    assert_eq!(
        vm.get_var(mapping[&1].unwrap(), "Child", "peer"),
        Some(RuntimeValue::None)
    );
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            collect(&entry.unwrap().path(), out);
        }
    } else if path.extension().is_some_and(|extension| extension == "ess") {
        out.push(path.to_owned());
    }
}

#[test]
fn installed_saved_instances_match_loaded_scripts() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA and SOE_TEST_SAVES required, skipping");
        return;
    };
    let archive = soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Misc.bsa")).unwrap();
    let mut paths = Vec::new();
    collect(Path::new(&saves), &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    let mut scripts = BTreeMap::new();
    for path in archive
        .files()
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".pex"))
    {
        let script = soe_papyrus::pex::parse(&archive.read(path).unwrap()).unwrap();
        scripts.insert(script.name().to_ascii_lowercase(), script);
    }
    let mut failures = Vec::new();
    let mut total_instances = 0;
    let mut total_arrays = 0;
    for path in &paths {
        let save = soe_save::Save::open(path).unwrap();
        let papyrus = save.papyrus().unwrap().unwrap();
        let mut vm = Vm::new();
        for definition in &papyrus.scripts {
            let name = papyrus
                .string(definition.name)
                .unwrap()
                .to_ascii_lowercase();
            if let Some(script) = scripts.get(&name) {
                vm.load(script.clone());
            }
        }
        let mut mapping = BTreeMap::new();
        for (id, script) in papyrus
            .instances
            .iter()
            .map(|instance| (instance.id, instance.script))
            .chain(
                papyrus
                    .references
                    .iter()
                    .map(|reference| (reference.id, reference.script)),
            )
        {
            let name = papyrus.string(script).unwrap();
            mapping.insert(id, Some(vm.create_object_of("corpus import", name)));
        }
        match papyrus.restore_instances(&mut vm, &mapping) {
            Ok(imported) => {
                assert_eq!(
                    imported.instances.len(),
                    papyrus.instances.len() + papyrus.references.len()
                );
                assert_eq!(imported.arrays.len(), papyrus.arrays.len());
                assert_eq!(vm.run(1), 0);
                total_instances += imported.instances.len();
                total_arrays += imported.arrays.len();
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }
    println!(
        "{} saves, {total_instances} restored instances, {total_arrays} shared arrays",
        paths.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
