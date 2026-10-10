// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SetOpen follows the CK contract and keeps door state in World.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"DOOR"), 0x800));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("HST21.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Hst21", "Form")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("target", "ObjectReference")
                .param("opened", "Bool")
                .local("result", "Var")
                .ins(
                    Opcode::CallMethod,
                    vec![
                        s("SetOpen"),
                        ident("target"),
                        ident("result"),
                        ident("opened"),
                    ],
                )
                .ins(Opcode::Return, vec![])
                .build(),
        )
        .function(
            FunctionBuilder::new("Default", "None")
                .global()
                .param("target", "ObjectReference")
                .local("result", "Var")
                .ins(
                    Opcode::CallMethod,
                    vec![s("SetOpen"), ident("target"), ident("result")],
                )
                .ins(Opcode::Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game
}

#[test]
fn hst_21_set_open_unlocks_and_repeated_close_keeps_the_reference() {
    let mut game = fixture();
    let door = id(0x801);
    let object = game.object_of_form(door);
    let mut state = game.world_mut();
    let reference = state.reference(door).unwrap();
    reference.locked = true;
    reference.lock_level = 75;
    drop(state);
    for opened in [true, false, true, false] {
        let thread = game
            .vm
            .call_global(
                "Hst21",
                "Run",
                &[Value::Object(object), Value::Bool(opened)],
            )
            .unwrap();
        game.run_until_idle(64);
        assert_eq!(game.vm.thread_result(thread), Some(Value::None));
        let world = game.world();
        let reference = world.reference_ref(door).unwrap();
        assert_eq!(reference.opened, opened);
        assert!(!reference.locked);
        assert_eq!(reference.lock_level, 0);
        assert_eq!(reference.base, Some(id(0x800)));
    }
}

#[test]
fn hst_21_set_open_rejects_missing_and_deleted_references() {
    let game = fixture();
    let mut world = game.world_mut();
    assert!(!world.set_reference_open(id(0xffff), true));
    world.reference(id(0x801)).unwrap().deleted = true;
    assert!(!world.set_reference_open(id(0x801), true));
    assert!(!world.reference_ref(id(0x801)).unwrap().opened);
}

#[test]
fn hst_21_set_open_defaults_to_open() {
    let mut game = fixture();
    let door = id(0x801);
    let object = game.object_of_form(door);
    let thread = game
        .vm
        .call_global("Hst21", "Default", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert!(game.world().reference_ref(door).unwrap().opened);
}
