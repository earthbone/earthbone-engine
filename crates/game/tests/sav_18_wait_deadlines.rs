// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{Opcode, Value};

fn wait_script() -> Vec<u8> {
    let mut script = ScriptBuilder::new("Sav18Waits", "");
    for (name, duration) in [("Real", 5.0), ("Menu", 5.0), ("Game", 1.1)] {
        let wait = match name {
            "Menu" => "WaitMenuMode",
            "Game" => "WaitGameTime",
            _ => "Wait",
        };
        script = script.function(
            FunctionBuilder::new(name, "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s(wait), ident("::nonevar"), f(duration)],
                )
                .ins(Opcode::Return, vec![i(1)])
                .build(),
        );
    }
    script.bytes()
}

fn game_with_wait_script() -> Game {
    let mut game = Game::new(LoadOrder::new());
    game.load_script(&wait_script()).unwrap();
    game
}

#[test]
fn sav_18_saved_wait_deadlines_keep_clock_domains_and_wake_once() {
    let mut game = game_with_wait_script();
    let real = game.vm.call_global("Sav18Waits", "Real", &[]).unwrap();
    let menu = game.vm.call_global("Sav18Waits", "Menu", &[]).unwrap();
    let game_time = game.vm.call_global("Sav18Waits", "Game", &[]).unwrap();
    game.run_until_idle(1000);
    let saved = game.save_state().unwrap();

    let mut restored = game_with_wait_script();
    restored.load_state(&saved).unwrap();
    restored.run_until_idle(1000);
    for thread in [real, menu, game_time] {
        assert!(matches!(
            restored.vm.thread_state(thread),
            Some(
                ThreadState::Sleeping(_)
                    | ThreadState::SleepingMenu(_)
                    | ThreadState::SleepingGame(_)
            )
        ));
        assert_eq!(restored.vm.thread_result(thread), None);
    }

    restored.tick_menu_mode(5.1, true);
    assert_eq!(restored.vm.thread_result(menu), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(real), None);
    assert_eq!(restored.vm.thread_result(game_time), None);

    restored.tick(4.9);
    assert_eq!(restored.vm.thread_result(real), None);
    assert_eq!(restored.vm.thread_result(game_time), None);
    restored.tick(0.2);
    assert_eq!(restored.vm.thread_result(real), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(game_time), None);

    restored.world_mut().clock.timescale = 3600.0;
    restored.tick(0.98);
    assert_eq!(restored.vm.thread_result(game_time), None);
    restored.tick(0.1);
    assert_eq!(restored.vm.thread_result(game_time), Some(Value::Int(1)));

    restored.tick(10.0);
    restored.run_until_idle(1000);
    assert_eq!(restored.vm.thread_result(real), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(menu), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(game_time), Some(Value::Int(1)));
}
