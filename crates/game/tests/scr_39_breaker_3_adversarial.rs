// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-39 warning behavior from the local Creation Kit contract.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const REFERENCE: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn impulse_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x803));
    let mut order = LoadOrder::new();
    order.add("Scr39ImpulseWarning.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn zero_length_impulse_reports_the_documented_papyrus_warning() {
    let mut game = impulse_game();
    let script = ScriptBuilder::new("Scr39ImpulseWarningProbe", "Form")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("reference", "ObjectReference")
                .param("zero", "Float")
                .param("magnitude", "Float")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("ApplyHavokImpulse"),
                        ident("reference"),
                        ident("result"),
                        ident("zero"),
                        ident("zero"),
                        ident("zero"),
                        ident("magnitude"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let reference = Value::Object(game.object_of_form(id(REFERENCE)));
    let thread = game
        .vm
        .call_global(
            "Scr39ImpulseWarningProbe",
            "Run",
            &[reference, Value::Float(0.0), Value::Float(5.0)],
        )
        .unwrap();
    game.run_until_idle(64);

    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert!(
        game.vm.log.contains("zero-length"),
        "CK documents a Papyrus warning for zero-length impulse vectors: {:?}",
        game.vm.log.messages()
    );
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}
