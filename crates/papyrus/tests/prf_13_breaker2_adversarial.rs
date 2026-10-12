// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Vm};
use soe_trace::{Entity, Profile};

fn wait_time(wait: &str, menu_seconds: f64, menu_hours: f64, live_hours: f64) -> u64 {
    Profile::disable();
    Profile::clear();
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
    Profile::enable();
    let thread = vm.call_global("WaitAudit", "Run", &[]).unwrap();
    vm.run(1);
    vm.advance_clocks(menu_seconds, menu_hours, true);
    vm.advance_clocks(0.5, live_hours, false);
    assert_eq!(
        vm.thread_state(thread),
        Some(soe_papyrus::vm::ThreadState::Runnable)
    );
    Profile::disable();
    let entity = Entity::new("script", "WaitAudit.Run", "WaitPatch.esp", "latent_wait");
    Profile::summary()[&entity].wait_ns
}

#[test]
fn prf_13_wait_clock_domains_do_not_charge_paused_menu_time() {
    assert_eq!(wait_time("Wait", 0.5, 0.0, 0.0), 500_000_000);
    assert_eq!(wait_time("WaitGameTime", 0.5, 10.0, 1.0), 500_000_000);
    assert_eq!(wait_time("WaitMenuMode", 0.5, 0.0, 0.0), 1_000_000_000);
}
