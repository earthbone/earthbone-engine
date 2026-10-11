// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::{NativeReturn, ThreadState};
use soe_papyrus::{Opcode, Value, Vm};
use soe_trace::{Entity, Profile};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_wait_attribution_accumulates_across_advances_and_menu_time() {
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    let bytes = ScriptBuilder::new("WaitAudit", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .local("result", "None")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s("Wait"), ident("result"), f(1.0)],
                )
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    vm.set_script_plugin("WaitAudit", "WaitPatch.esp");

    Profile::enable();
    vm.call_global("WaitAudit", "Run", &[]).unwrap();
    assert_eq!(vm.run(1), 1);
    vm.advance_clocks(0.4, 0.0, false);
    vm.advance_clocks(0.6, 0.0, false);
    Profile::disable();

    let entity = Entity::new("script", "WaitAudit.Run", "WaitPatch.esp", "latent_wait");
    let stats = &Profile::summary()[&entity];
    assert_eq!(stats.waits, 1);
    assert_eq!(stats.wait_ns, 1_000_000_000);
}

fn waiter(wait: &str) -> Vm {
    let bytes = ScriptBuilder::new("WaitAudit", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .local("result", "None")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s(wait), ident("result"), f(1.0)],
                )
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    vm.set_script_plugin("WaitAudit", "WaitPatch.esp");
    vm
}

#[test]
fn prf_13_wait_attribution_clips_deadlines_in_each_clock_domain() {
    let _guard = LOCK.lock().unwrap();
    for (wait, expected_ns) in [
        ("Wait", 1_250_000_000),
        ("WaitMenuMode", 1_000_000_000),
        ("WaitGameTime", 1_500_000_000),
    ] {
        Profile::disable();
        Profile::clear();
        let mut vm = waiter(wait);
        Profile::enable();
        let thread = vm.call_global("WaitAudit", "Run", &[]).unwrap();
        assert_eq!(vm.run(1), 1);
        vm.advance_clocks(0.25, 100.0, true);
        vm.advance_clocks(0.5, 0.25, false);
        vm.advance_clocks(2.0, 2.0, false);
        assert_eq!(vm.thread_state(thread), Some(ThreadState::Runnable));
        vm.advance_clocks(10.0, 10.0, false);
        Profile::disable();
        let entity = Entity::new("script", "WaitAudit.Run", "WaitPatch.esp", "latent_wait");
        let stats = &Profile::summary()[&entity];
        assert_eq!(stats.waits, 1, "{wait} counted clock advances as waits");
        assert_eq!(stats.wait_ns, expected_ns, "{wait} exceeded its deadline");
    }
}

#[test]
fn prf_13_native_latent_wait_stops_charging_after_resolution() {
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    let mut vm = waiter("Deferred");
    vm.natives.register_global(
        "Utility",
        "Deferred",
        Box::new(|call| NativeReturn::Latent(call.defer())),
    );
    Profile::enable();
    let thread = vm.call_global("WaitAudit", "Run", &[]).unwrap();
    vm.run(1);
    let Some(ThreadState::Latent(latent)) = vm.thread_state(thread) else {
        panic!("expected native latent stack");
    };
    vm.advance_clocks(0.25, 100.0, true);
    vm.advance_clocks(0.5, 0.25, false);
    assert!(vm.resolve_latent(latent, Value::None));
    vm.advance_clocks(10.0, 10.0, false);
    vm.run(1);
    Profile::disable();
    let entity = Entity::new("script", "WaitAudit.Run", "WaitPatch.esp", "latent_wait");
    let stats = &Profile::summary()[&entity];
    assert_eq!(stats.waits, 1);
    assert_eq!(stats.wait_ns, 750_000_000);
    assert_eq!(vm.thread_state(thread), Some(ThreadState::Done));
}
