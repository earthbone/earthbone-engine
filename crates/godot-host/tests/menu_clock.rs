// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Menu clock checks through the same host coordinator used by PlayerNode.

use soe_game::Game;
use soe_godot_host::ui::MenuState;
use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{
    Opcode::{CallStatic, Return},
    Value,
};

#[test]
fn native_menu_pauses_world_and_ordinary_waits_while_menu_waits_continue() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut script = ScriptBuilder::new("MenuClockProbe", "");
    for name in ["Wait", "WaitMenuMode", "WaitGameTime"] {
        script = script.function(
            FunctionBuilder::new(name, "Int")
                .global()
                .ins(
                    CallStatic,
                    vec![s("Utility"), s(name), ident("::nonevar"), f(1.0)],
                )
                .ins(Return, vec![i(7)])
                .build(),
        );
    }
    game.load_script(&script.bytes()).unwrap();
    let wait = game.vm.call_global("MenuClockProbe", "Wait", &[]).unwrap();
    let menu_wait = game
        .vm
        .call_global("MenuClockProbe", "WaitMenuMode", &[])
        .unwrap();
    let game_wait = game
        .vm
        .call_global("MenuClockProbe", "WaitGameTime", &[])
        .unwrap();
    game.run_until_idle(64);
    let mut menu = MenuState::default();
    assert!(menu.open("Menu"));
    assert!(!menu.open("Second menu"));
    let before = game.world().clock.clone();
    menu.tick(&mut game, 1000.0);
    assert_eq!(game.world().clock, before);
    assert_eq!(game.vm.thread_result(menu_wait), Some(Value::Int(7)));
    assert_eq!(game.vm.thread_result(wait), None);
    assert_eq!(game.vm.thread_result(game_wait), None);
    assert!(menu.close());
    assert!(!menu.close());
    menu.tick(&mut game, 1.1);
    assert_eq!(game.vm.thread_result(wait), Some(Value::Int(7)));
    assert_eq!(game.vm.thread_result(game_wait), None);
    game.world_mut().clock.timescale = 3600.0;
    menu.tick(&mut game, 1.1);
    assert_eq!(game.vm.thread_result(game_wait), Some(Value::Int(7)));
}
