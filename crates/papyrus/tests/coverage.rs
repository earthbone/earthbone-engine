// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::coverage::{FunctionKind, Summary};
use soe_papyrus::{Opcode::*, Value, Vm};

#[test]
fn executed_bodies_distinguish_states_accessors_and_inheritance() {
    let parent = ScriptBuilder::new("Parent", "")
        .function(FunctionBuilder::new("Inherited", "None").build())
        .bytes();
    let script = ScriptBuilder::new("Child", "Parent")
        .function(
            FunctionBuilder::new("Run", "None")
                .ins(
                    CallMethod,
                    vec![s("Inherited"), ident("self"), ident("::nonevar")],
                )
                .ins(PropGet, vec![s("Value"), ident("self"), ident("v")])
                .ins(PropSet, vec![s("Value"), ident("self"), i(7)])
                .ins(
                    CallMethod,
                    vec![s("GotoState"), ident("self"), ident("::nonevar"), s("Busy")],
                )
                .ins(
                    CallMethod,
                    vec![s("Run"), ident("self"), ident("::nonevar")],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .function_in("Busy", FunctionBuilder::new("Run", "None").build())
        .function(FunctionBuilder::new("Uncalled", "None").build())
        .function(FunctionBuilder::new("Native", "None").native().build())
        .property(
            "Value",
            "Int",
            Some(
                FunctionBuilder::new("get", "Int")
                    .ins(Return, vec![i(3)])
                    .build(),
            ),
            Some(
                FunctionBuilder::new("set", "None")
                    .param("value", "Int")
                    .build(),
            ),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&parent).unwrap();
    vm.load_bytes(&script).unwrap();
    let object = vm.create_object("object");
    vm.attach(object, "Child").unwrap();
    vm.start_coverage();
    vm.call(object, "Run", &[]).unwrap();
    assert_eq!(vm.run(0), 0);
    let queued = vm.coverage_report().unwrap();
    assert_eq!(
        queued.scripts["child"],
        Summary {
            executed: 0,
            total: 5
        }
    );
    assert!(queued.functions.iter().all(|f| f.objects.is_empty()));
    assert!(vm.run_until_idle(100));
    let report = vm.stop_coverage().unwrap();
    assert_eq!(
        report.scripts["child"],
        Summary {
            executed: 4,
            total: 5
        }
    );
    assert_eq!(
        report.scripts["parent"],
        Summary {
            executed: 1,
            total: 1
        }
    );
    assert!(report.functions.iter().any(
        |f| matches!(&f.id.kind, FunctionKind::Getter(p) if p == "value")
            && f.objects.contains(&Some(object))
    ));
    assert_eq!(report.scripts["child"].percent(), Some(80.0));
    assert_eq!(Summary::default().percent(), None);
    assert!(vm.coverage_report().is_none());
}

#[test]
fn coverage_does_not_change_hashes_or_saves_and_resumed_frames_count() {
    let script = ScriptBuilder::new("Sleep", "")
        .function(
            FunctionBuilder::new("Wait", "None")
                .global()
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("Wait"), ident("::nonevar"), i(1)],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&script).unwrap();
    let thread = vm.call_global("Sleep", "Wait", &[]).unwrap();
    let before = vm.save_state().unwrap();
    let hash = vm.state_hash();
    vm.start_coverage();
    assert_eq!(before, vm.save_state().unwrap());
    assert_eq!(hash, vm.state_hash());
    vm.run_until_idle(10);
    let saved = vm.save_state().unwrap();
    let covered_hash = vm.state_hash();
    assert!(vm.coverage_report().unwrap().functions[0]
        .objects
        .contains(&None));
    vm.stop_coverage();
    assert_eq!(saved, vm.save_state().unwrap());
    assert_eq!(covered_hash, vm.state_hash());
    vm.start_coverage();
    vm.load_state(&saved).unwrap();
    assert!(vm.coverage_report().unwrap().functions[0]
        .objects
        .is_empty());
    vm.advance(1.0);
    vm.run_until_idle(10);
    assert_eq!(vm.thread_result(thread), Some(Value::None));
    assert_eq!(vm.coverage_report().unwrap().scripts["sleep"].executed, 1);
    vm.load_bytes(&script).unwrap();
    assert!(
        vm.coverage_report().is_none(),
        "loading changed the denominator"
    );
}

#[test]
fn old_attached_bodies_cannot_cover_reloaded_definitions() {
    let script = ScriptBuilder::new("Script", "")
        .function(FunctionBuilder::new("Run", "None").build())
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&script).unwrap();
    let old = vm.create_object("old");
    vm.attach(old, "Script").unwrap();
    vm.load_bytes(&script).unwrap();
    vm.start_coverage();
    vm.call(old, "Run", &[]).unwrap();
    vm.run_until_idle(10);
    assert_eq!(vm.coverage_report().unwrap().scripts["script"].executed, 0);
    let new = vm.create_object("new");
    vm.attach(new, "Script").unwrap();
    vm.call(new, "Run", &[]).unwrap();
    vm.run_until_idle(10);
    assert_eq!(vm.coverage_report().unwrap().scripts["script"].executed, 1);
}
