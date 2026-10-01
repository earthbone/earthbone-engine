// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

#[test]
fn scr_23_failed_move_preserves_every_player_transform_component() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), 0x800).zsub(T(b"EDID"), "HomeCell"));
    let mut order = LoadOrder::new(); order.add("Scr23.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).unwrap();
        reference.position = [1.0, 2.0, 3.0];
        reference.rotation = [0.1, 0.2, 0.3];
        reference.cell = Some(id(0x800));
    }
    let script = "Scr23MissingMoveProbe";
    let function = FunctionBuilder::new("Run", "Float").global().param("destination", "String").local("result", "Var")
        .ins(CallStatic, vec![s("Debug"), s("PlayerMoveToAndWait"), ident("result"), ident("destination")])
        .ins(Return, vec![ident("result")]).build();
    game.load_script(&ScriptBuilder::new(script, "Form").function(function).bytes()).unwrap();
    let thread = game.vm.call_global(script, "Run", &[Value::string("Missing")]).unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(0.0)));
    let world = game.world();
    let reference = world.reference_ref(player).unwrap();
    assert_eq!(reference.position, [1.0, 2.0, 3.0]);
    assert_eq!(reference.rotation, [0.1, 0.2, 0.3]);
    assert_eq!(reference.cell, Some(id(0x800)));
}
