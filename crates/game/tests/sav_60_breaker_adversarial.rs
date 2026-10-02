// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::{
    asm::{FunctionBuilder, ScriptBuilder, i, none},
    value::{ArrayRef, ElementKind, Value},
};

fn script(name: &str, result: i32, incompatible: bool) -> Vec<u8> {
    let mut builder = ScriptBuilder::new(name, "")
        .variable("shared", "Int[]", none())
        .function(
            FunctionBuilder::new("Run", "Int")
                .ins(soe_papyrus::pex::Opcode::Return, vec![i(result)])
                .build(),
        );
    if incompatible {
        builder = builder.variable("changed", "Int", i(1));
    }
    builder.bytes()
}

fn game(scripts: &[Vec<u8>]) -> Game {
    let mut game = Game::new(LoadOrder::new());
    for script in scripts {
        game.load_script(script).unwrap();
    }
    game
}

#[test]
fn sav_60_partial_dependency_rebind_is_atomic_and_resumes_once() {
    let first = script("RecoverFirst", 37, false);
    let second = script("RecoverSecond", 82, false);
    let bad_second = script("RecoverSecond", 82, true);
    let mut source = game(&[first.clone(), second.clone()]);
    let first_owner = source.vm.create_object("first owner");
    let second_owner = source.vm.create_object("second owner");
    source.vm.attach(first_owner, "RecoverFirst").unwrap();
    source.vm.attach(second_owner, "RecoverSecond").unwrap();
    let shared = ArrayRef::new(ElementKind::Int, 2);
    for (owner, name) in [
        (first_owner, "RecoverFirst"),
        (second_owner, "RecoverSecond"),
    ] {
        source
            .vm
            .set_var(owner, name, "shared", Value::Array(shared.clone()))
            .unwrap();
    }
    let first_thread = source.vm.call_global("RecoverFirst", "Run", &[]).unwrap();
    let second_thread = source.vm.call_global("RecoverSecond", "Run", &[]).unwrap();
    let saved = source.save_state().unwrap();

    let mut partial = game(&[]);
    partial.load_state(&saved).unwrap();
    partial.load_script(&first).unwrap();
    assert!(partial.vm.has_suspended_state());
    assert_eq!(partial.vm.run(100), 0);
    assert_eq!(
        partial.vm.get_var(first_owner, "RecoverFirst", "shared"),
        None
    );

    partial.load_script(&bad_second).unwrap();
    assert!(partial.vm.has_suspended_state());
    assert_eq!(partial.vm.live_threads(), 0);
    assert_eq!(partial.vm.run(100), 0);

    partial.load_script(&second).unwrap();
    assert!(!partial.vm.has_suspended_state());
    let Value::Array(left) = partial
        .vm
        .get_var(first_owner, "RecoverFirst", "shared")
        .unwrap()
    else {
        panic!("first dependency variable should be restored")
    };
    let Value::Array(right) = partial
        .vm
        .get_var(second_owner, "RecoverSecond", "shared")
        .unwrap()
    else {
        panic!("second dependency variable should be restored")
    };
    assert!(Rc::ptr_eq(&left.items, &right.items));
    assert!(partial.vm.run(100) > 0);
    assert_eq!(partial.vm.thread_result(first_thread), Some(Value::Int(37)));
    assert_eq!(
        partial.vm.thread_result(second_thread),
        Some(Value::Int(82))
    );
    assert_eq!(partial.vm.run(100), 0);
    assert_eq!(partial.vm.live_threads(), 0);
    assert!(!partial.vm.resume_suspended_state().unwrap());
}
