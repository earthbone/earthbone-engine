// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

#[test]
fn valid_fire_does_not_finish_as_an_unsupported_operation() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x100));
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 0x200)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), [0; 100]),
    );
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), 0x300).sub(Tag::new(b"NAME"), 0x100_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr53FireContract.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Scr53FireContractProbe", "Weapon")
        .function(
            FunctionBuilder::new("Run", "None")
                .param("source", "Form")
                .ins(
                    CallMethod,
                    vec![s("Fire"), ident("self"), ident("result"), ident("source")],
                )
                .ins(Return, vec![])
                .local("result", "None")
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let weapon = game.object_of_form(soe_esm::GlobalFormId {
        file: 0,
        object: 0x200,
    });
    game.vm.attach(weapon, "Scr53FireContractProbe").unwrap();
    let source = Value::Object(game.object_of_form(soe_esm::GlobalFormId {
        file: 0,
        object: 0x300,
    }));
    let thread = game.vm.call(weapon, "Run", &[source]).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));

    assert!(
        !game.vm.log.contains("unsupported weapon fire behavior"),
        "valid Weapon.Fire must execute rather than report an unsupported operation"
    );
}
