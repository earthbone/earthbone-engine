// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Vm};
use soe_trace::{Entity, Profile};

#[test]
fn prf_13_wait_attribution_accumulates_across_advances_and_menu_time() {
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
