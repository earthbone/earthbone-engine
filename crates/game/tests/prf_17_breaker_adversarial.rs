// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;
use std::thread::ThreadId;

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::{NativeReturn, Value};

#[test]
fn prf_17_normal_papyrus_dispatch_stays_on_the_calling_thread() {
    let caller = std::thread::current().id();
    let observed = Rc::new(RefCell::new(None::<ThreadId>));
    let mut game = Game::new(LoadOrder::new());
    let native_observed = Rc::clone(&observed);

    game.add_natives(move |vm, _world| {
        vm.natives.register_global(
            "WorkerProbe",
            "RecordThread",
            Box::new(move |_| {
                *native_observed.borrow_mut() = Some(std::thread::current().id());
                NativeReturn::Value(Value::None)
            }),
        );
    });
    game.load_script(
        &ScriptBuilder::new("WorkerProbe", "")
            .function(
                FunctionBuilder::new("Run", "None")
                    .global()
                    .ins(
                        CallStatic,
                        vec![s("WorkerProbe"), s("RecordThread"), ident("::nonevar")],
                    )
                    .build(),
            )
            .bytes(),
    )
    .unwrap();

    game.vm.call_global("WorkerProbe", "Run", &[]).unwrap();
    assert!(game.run_until_idle(8));

    let observed = observed.borrow().expect("native ran");
    assert_eq!(observed, caller, "normal dispatch left the caller thread");
}

#[test]
fn prf_17_game_tick_runs_papyrus_tasklets_on_a_worker_thread() {
    let caller = std::thread::current().id();
    let observed = Rc::new(RefCell::new(None::<ThreadId>));
    let mut game = Game::new(LoadOrder::new());
    let native_observed = Rc::clone(&observed);

    game.add_natives(move |vm, _world| {
        vm.natives.register_global(
            "WorkerProbe",
            "RecordThread",
            Box::new(move |_| {
                *native_observed.borrow_mut() = Some(std::thread::current().id());
                NativeReturn::Value(Value::None)
            }),
        );
    });
    game.load_script(
        &ScriptBuilder::new("WorkerProbe", "")
            .function(
                FunctionBuilder::new("Run", "None")
                    .global()
                    .ins(
                        CallStatic,
                        vec![s("WorkerProbe"), s("RecordThread"), ident("::nonevar")],
                    )
                    .build(),
            )
            .bytes(),
    )
    .unwrap();

    game.vm.call_global("WorkerProbe", "Run", &[]).unwrap();
    game.tick(1.0 / 60.0);

    let observed = observed.borrow().expect("native ran");
    assert_ne!(observed, caller, "Papyrus tasklet ran on the caller thread");
}
