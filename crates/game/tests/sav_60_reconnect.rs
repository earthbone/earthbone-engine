// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::{
    asm::{FunctionBuilder, ScriptBuilder, i},
    value::{ArrayRef, ElementKind, Value},
    vm::ThreadState,
};

fn script(return_value: i32, extra_variable: bool) -> Vec<u8> {
    let mut builder = ScriptBuilder::new("Recoverable", "")
        .variable("left", "Int[]", soe_papyrus::asm::none())
        .variable("right", "Int[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("Run", "Int")
                .ins(soe_papyrus::pex::Opcode::Return, vec![i(return_value)])
                .build(),
        );
    if extra_variable {
        builder = builder.variable("replacementOnly", "Int", i(1));
    }
    builder.bytes()
}

fn game(pex: Option<&[u8]>) -> Game {
    let mut game = Game::new(LoadOrder::new());
    if let Some(pex) = pex {
        game.load_script(pex).unwrap();
    }
    game
}

#[test]
fn sav_60_removed_script_state_reconnects_once_with_shared_array_identity() {
    let compatible = script(37, false);
    let incompatible = script(37, true);
    let mut source = game(Some(&compatible));
    let owner = source.vm.create_object("saved owner");
    source.vm.attach(owner, "Recoverable").unwrap();
    let shared = ArrayRef::new(ElementKind::Int, 2);
    source
        .vm
        .set_var(owner, "Recoverable", "left", Value::Array(shared.clone()))
        .unwrap();
    source
        .vm
        .set_var(owner, "Recoverable", "right", Value::Array(shared))
        .unwrap();
    let thread = source.vm.call_global("Recoverable", "Run", &[]).unwrap();
    let saved = source.save_state().unwrap();

    let mut absent = game(None);
    absent.load_state(&saved).unwrap();
    assert!(absent.vm.has_suspended_state());
    assert_eq!(absent.vm.run(100), 0);
    let saved_while_absent = absent.save_state().unwrap();

    let mut restored = game(None);
    restored.load_state(&saved_while_absent).unwrap();
    restored.load_script(&incompatible).unwrap();
    assert!(restored.vm.has_suspended_state());
    assert_eq!(restored.vm.run(100), 0);
    assert_eq!(restored.vm.live_threads(), 0);
    assert_eq!(restored.vm.get_var(owner, "Recoverable", "left"), None);

    restored.load_script(&compatible).unwrap();
    assert!(!restored.vm.has_suspended_state());
    assert!(!restored.vm.resume_suspended_state().unwrap());
    assert_eq!(restored.vm.live_threads(), 1);
    let Value::Array(left) = restored.vm.get_var(owner, "Recoverable", "left").unwrap() else {
        panic!("left variable should remain an array")
    };
    let Value::Array(right) = restored.vm.get_var(owner, "Recoverable", "right").unwrap() else {
        panic!("right variable should remain an array")
    };
    assert!(Rc::ptr_eq(&left.items, &right.items));

    assert!(restored.vm.run(100) > 0);
    assert_eq!(restored.vm.thread_state(thread), Some(ThreadState::Done));
    assert_eq!(restored.vm.thread_result(thread), Some(Value::Int(37)));
    assert_eq!(restored.vm.run(100), 0);
    assert_eq!(restored.vm.live_threads(), 0);
}
