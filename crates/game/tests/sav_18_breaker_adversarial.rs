// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::ThreadState;
use soe_papyrus::{Opcode, Value};

fn script() -> Vec<u8> {
    let mut builder = ScriptBuilder::new("Sav18SplitClock", "");
    for (name, native, duration) in [
        ("Real", "Wait", 3.0),
        ("Menu", "WaitMenuMode", 3.0),
        ("Game", "WaitGameTime", 0.5),
    ] {
        builder = builder.function(
            FunctionBuilder::new(name, "Int")
                .ins(
                    Opcode::CallStatic,
                    vec![s("Utility"), s(native), ident("::nonevar"), f(duration)],
                )
                .ins(Opcode::Return, vec![i(1)])
                .build(),
        );
    }
    builder.bytes()
}

fn game() -> Game {
    let mut game = Game::new(LoadOrder::new());
    game.load_script(&script()).unwrap();
    game
}

#[test]
fn sav_18_round_trip_keeps_divergent_wait_clock_origins() {
    let mut source = game();
    source.vm.advance_clocks(100.0, 5.0, false);
    let real = source.vm.call_global("Sav18SplitClock", "Real", &[]).unwrap();
    let menu = source.vm.call_global("Sav18SplitClock", "Menu", &[]).unwrap();
    let game_time = source.vm.call_global("Sav18SplitClock", "Game", &[]).unwrap();
    source.run_until_idle(100);
    assert!(matches!(source.vm.thread_state(real), Some(ThreadState::Sleeping(_))));
    assert!(matches!(source.vm.thread_state(menu), Some(ThreadState::SleepingMenu(_))));
    assert!(matches!(source.vm.thread_state(game_time), Some(ThreadState::SleepingGame(_))));
    let saved = source.save_state().unwrap();

    let mut restored = game();
    restored.load_state(&saved).unwrap();
    restored.run_until_idle(100);
    assert_eq!(restored.vm.thread_result(real), None);
    assert_eq!(restored.vm.thread_result(menu), None);
    assert_eq!(restored.vm.thread_result(game_time), None);

    restored.tick_menu_mode(3.1, true);
    assert_eq!(restored.vm.thread_result(menu), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(real), None);
    assert_eq!(restored.vm.thread_result(game_time), None);

    restored.vm.advance_clocks(2.9, 0.0, false);
    restored.run_until_idle(100);
    assert_eq!(restored.vm.thread_result(real), None);
    restored.vm.advance_clocks(0.2, 0.0, false);
    restored.run_until_idle(100);
    assert_eq!(restored.vm.thread_result(real), Some(Value::Int(1)));
    assert_eq!(restored.vm.thread_result(game_time), None);

    restored.vm.advance_clocks(0.0, 0.49, false);
    restored.run_until_idle(100);
    assert_eq!(restored.vm.thread_result(game_time), None);
    restored.vm.advance_clocks(0.0, 0.02, false);
    restored.run_until_idle(100);
    assert_eq!(restored.vm.thread_result(game_time), Some(Value::Int(1)));
    for thread in [real, menu, game_time] {
        assert_eq!(restored.vm.thread_result(thread), Some(Value::Int(1)));
    }
}
