// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Vm};
use soe_trace::{Entity, Profile};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_concurrent_latent_waits_accumulate_per_function_and_stop_at_deadline() {
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
                    vec![s("Utility"), s("Wait"), ident("result"), f(0.5)],
                )
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    let mut vm = Vm::new();
    vm.load_bytes(&bytes).unwrap();
    vm.set_script_plugin("WaitAudit", "WaitPatch.esp");

    Profile::enable();
    let first = vm.call_global("WaitAudit", "Run", &[]).unwrap();
    let second = vm.call_global("WaitAudit", "Run", &[]).unwrap();
    assert_eq!(vm.run(2), 2);
    vm.advance_clocks(0.25, 0.0, true);
    vm.advance_clocks(0.25, 0.0, false);
    vm.advance_clocks(0.25, 0.0, false);
    assert_eq!(vm.thread_state(first), Some(soe_papyrus::vm::ThreadState::Runnable));
    assert_eq!(vm.thread_state(second), Some(soe_papyrus::vm::ThreadState::Runnable));
    vm.advance_clocks(4.0, 0.0, false);
    Profile::disable();

    let entity = Entity::new("script", "WaitAudit.Run", "WaitPatch.esp", "latent_wait");
    let stats = &Profile::summary()[&entity];
    assert_eq!(stats.waits, 2);
    assert_eq!(stats.wait_ns, 1_500_000_000);
}
