// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_game::{Game};
use soe_esm::LoadOrder;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

#[test]
fn scr_21_repeated_debug_toggles_restore_the_starting_switches() {
    let mut game = Game::new(LoadOrder::new());
    let script = "Scr21ToggleProbe";
    let function = FunctionBuilder::new("Run", "None").global()
        .ins(CallStatic, vec![s("Debug"), s("ToggleAI"), ident("::nonevar")])
        .ins(CallStatic, vec![s("Debug"), s("ToggleAI"), ident("::nonevar")])
        .ins(CallStatic, vec![s("Debug"), s("ToggleCollisions"), ident("::nonevar")])
        .ins(CallStatic, vec![s("Debug"), s("ToggleCollisions"), ident("::nonevar")])
        .ins(Return, vec![soe_papyrus::asm::none()]).build();
    game.load_script(&ScriptBuilder::new(script, "Form").function(function).bytes()).unwrap();
    game.vm.call_global(script, "Run", &[]).unwrap();
    game.run_until_idle(64);
    assert!(game.world().debug_ai_enabled);
    assert!(game.world().debug_collisions_enabled);
    assert!(game.vm.log.messages().is_empty());
}

#[test]
fn scr_21_god_mode_argument_is_applied_on_each_call() {
    let mut game = Game::new(LoadOrder::new());
    let script = "Scr21GodModeProbe";
    let function = FunctionBuilder::new("Run", "None").global().param("enabled", "Bool")
        .ins(CallStatic, vec![s("Debug"), s("SetGodMode"), ident("::nonevar"), ident("enabled")])
        .ins(Return, vec![soe_papyrus::asm::none()]).build();
    game.load_script(&ScriptBuilder::new(script, "Form").function(function).bytes()).unwrap();
    for (value, expected) in [(true, true), (false, false)] {
        game.vm.call_global(script, "Run", &[Value::Bool(value)]).unwrap();
        game.run_until_idle(64);
        assert_eq!(game.world().debug_god_mode, expected);
    }
}
