// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::CallMethod;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x800;
const OTHER: u32 = 0x801;
const REF: u32 = 0x802;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reset_probe(game: &mut Game, cell: u32) {
    let script = "Scr19ResetProbe";
    let bytes = ScriptBuilder::new(script, "Form")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("cell", "Cell")
                .ins(
                    CallMethod,
                    vec![s("Reset"), ident("cell"), ident("::nonevar")],
                )
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let cell_object = game.object_of_form(id(cell));
    game.vm
        .call_global(script, "Run", &[soe_papyrus::Value::Object(cell_object)])
        .unwrap();
    game.run_until_idle(64);
}

#[test]
fn scr_19_pending_reset_round_trips_and_does_not_mark_another_cell() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"CELL"), OTHER));
    plugin.push_top(Record::new(T(b"STAT"), 0x803));
    let mut reference = Record::new(T(b"REFR"), REF).sub(T(b"NAME"), 0x803_u32.to_le_bytes());
    reference.placement.cell = Some(CELL);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("Scr19.esm", plugin).unwrap();
    let mut game = Game::new(order);
    reset_probe(&mut game, CELL);
    game.world_mut().reference(id(REF)).unwrap().enabled = false;
    let saved = game.world_mut().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert!(!game.world_mut().cell_is_attached(id(CELL)));
    game.world_mut().load_cell(id(CELL));
    assert!(game.world_mut().reference(id(REF)).unwrap().enabled);
    game.world_mut().unload_cell(id(CELL));
    game.world_mut().load_cell(id(CELL));
    assert!(game.world_mut().reset_cell(id(OTHER)).is_ok());
}
