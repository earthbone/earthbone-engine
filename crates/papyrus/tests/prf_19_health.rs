// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-19 scheduler tests against assembled scripts and public diagnostics.

use soe_papyrus::asm::{f, i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::{NativeReturn, ThreadState};
use soe_papyrus::{ObjectId, Opcode, Value, Vm};

fn fixture() -> (Vm, ObjectId) {
    let mut builder = ScriptBuilder::new("Health", "");
    builder = builder.function(
        FunctionBuilder::new("Busy", "None")
            .ins(Opcode::Jmp, vec![i(0)])
            .build(),
    );
    for wait in ["Wait", "WaitMenuMode", "WaitGameTime", "Deferred"] {
        builder = builder.function(
            FunctionBuilder::new(wait, "None")
                .ins(
                    Opcode::CallStatic,
                    vec![ident("Utility"), ident(wait), ident("::nonevar"), f(1.0)],
                )
                .ins(Opcode::Jmp, vec![i(-1)])
                .build(),
        );
    }
    builder = builder.function(
        FunctionBuilder::new("OnItemAdded", "None")
            .ins(Opcode::Return, vec![])
            .build(),
    );
    let mut vm = Vm::new();
    vm.load_bytes(&builder.bytes()).unwrap();
    let object = vm.create_object("health");
    vm.attach(object, "Health").unwrap();
    (vm, object)
}

#[test]
fn prf_19_backlog_excludes_sleepers_and_tracks_the_oldest_unserved_stack() {
    let (mut vm, object) = fixture();
    vm.call(object, "Busy", &[]).unwrap();
    vm.call(object, "Busy", &[]).unwrap();
    vm.run(1);
    assert_eq!(vm.health().backlog, 2);
    vm.advance(0.25);
    vm.run(1);
    assert_eq!(vm.health().oldest_wait_seconds, 0.25);
    assert_eq!(vm.health_rows()[0].health.backlog, 2);
    assert_eq!(vm.health().suspended_stacks, 0);
}

#[test]
fn prf_19_wait_loop_reports_suspension_elapsed_wait_and_frame_running_time() {
    let (mut vm, object) = fixture();
    let thread = vm.call(object, "Wait", &[]).unwrap();
    vm.run(100);
    assert!(matches!(
        vm.thread_state(thread),
        Some(ThreadState::Sleeping(_))
    ));
    assert_eq!(vm.health().suspended_stacks, 1);
    assert_eq!(vm.health().backlog, 0);
    assert!(vm.health().running_ns > 0);
    vm.advance(0.4);
    vm.run(100);
    assert_eq!(vm.health().waiting_seconds, 0.4);
    assert_eq!(vm.health().running_ns, 0);
    vm.advance(0.8);
    vm.run(100);
    assert!((vm.health().waiting_seconds - 0.6).abs() < 1e-9);
    assert_eq!(vm.health_rows()[0].health.suspended_stacks, 1);
    assert!(vm.health().running_ns > 0);
}

#[test]
fn prf_19_wait_domains_include_menu_time_and_game_hour_deadlines() {
    let (mut vm, object) = fixture();
    for name in ["Wait", "WaitMenuMode", "WaitGameTime"] {
        vm.call(object, name, &[]).unwrap();
    }
    vm.run(100);
    vm.advance_clocks(2.0, 100.0, true);
    assert_eq!(vm.health().suspended_stacks, 2);
    assert_eq!(vm.health().waiting_seconds, 5.0);
    vm.run(100);
    vm.advance_clocks(2.0, 2.0, false);
    assert!((vm.health().waiting_seconds - 3.0).abs() < 1e-9);
}

#[test]
fn prf_19_external_latent_native_resolves_without_becoming_backlog() {
    let (mut vm, object) = fixture();
    vm.natives.register_global(
        "Utility",
        "Deferred",
        Box::new(|call| NativeReturn::Latent(call.defer())),
    );
    let thread = vm.call(object, "Deferred", &[]).unwrap();
    vm.run(100);
    let Some(ThreadState::Latent(id)) = vm.thread_state(thread) else {
        panic!("expected latent stack")
    };
    vm.advance(0.5);
    assert_eq!(vm.health().waiting_seconds, 0.5);
    assert_eq!(vm.health().backlog, 0);
    assert!(vm.resolve_latent(id, Value::None));
    assert_eq!(vm.health().suspended_stacks, 0);
}

#[test]
fn prf_19_queued_event_counts_drain_at_the_first_instruction_and_keep_peak() {
    let (mut vm, object) = fixture();
    for _ in 0..500 {
        vm.send_event(object, "OnItemAdded", &[]);
    }
    let event = &vm.health().events["onitemadded"];
    assert_eq!((event.queued, event.peak_per_second), (500, 500));
    vm.run(1);
    assert_eq!(vm.health().events["onitemadded"].queued, 499);
    vm.run(1000);
    assert_eq!(vm.health().events["onitemadded"].queued, 0);
    vm.advance(2.0);
    vm.send_event(object, "OnItemAdded", &[]);
    assert_eq!(vm.health().events["onitemadded"].peak_per_second, 500);
}

#[test]
fn prf_19_health_queries_and_save_load_do_not_change_execution_state() {
    let (mut vm, object) = fixture();
    vm.call(object, "Wait", &[]).unwrap();
    vm.run(100);
    let bytes = vm.save_state().unwrap();
    let _ = vm.health();
    let _ = vm.health_rows();
    assert_eq!(vm.save_state().unwrap(), bytes);
    vm.load_state(&bytes).unwrap();
    assert_eq!(vm.health().running_ns, 0);
    assert_eq!(vm.health().suspended_stacks, 1);
    assert!(vm.health().events.is_empty());
}

#[test]
fn prf_19_suspended_nested_stack_is_visible_in_each_script_and_total_once() {
    let mut vm = Vm::new();
    vm.load_bytes(
        &ScriptBuilder::new("Outer", "")
            .function(
                FunctionBuilder::new("Run", "None")
                    .ins(
                        Opcode::CallStatic,
                        vec![ident("Inner"), ident("Pause"), ident("::nonevar")],
                    )
                    .ins(Opcode::Return, vec![])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    vm.load_bytes(
        &ScriptBuilder::new("Inner", "")
            .function(
                FunctionBuilder::new("Pause", "None")
                    .global()
                    .ins(
                        Opcode::CallStatic,
                        vec![ident("Utility"), ident("Wait"), ident("::nonevar"), f(1.0)],
                    )
                    .ins(Opcode::Return, vec![])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    vm.call_global("Outer", "Run", &[]).unwrap();
    vm.run(100);
    let rows = vm.health_rows();
    assert_eq!(
        rows.iter()
            .find(|row| row.script == "outer")
            .unwrap()
            .health
            .suspended_stacks,
        1
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.script == "inner")
            .unwrap()
            .health
            .suspended_stacks,
        1
    );
    assert_eq!(vm.health().suspended_stacks, 1);
}
