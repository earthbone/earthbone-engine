// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

#[test]
fn cell_owner_contract_inventory_covers_ck_typed_setters_and_clear_alias() {
    let audit = include_str!("../../../docs/reference-audit.md");
    for contract in [
        "`Cell.SetActorOwner`",
        "`Cell.SetFactionOwner`",
        "`Alias.Clear`",
    ] {
        assert!(audit.contains(contract), "missing audit for {contract}");
    }
    assert!(audit.contains("SetActorOwner_-_Cell.wikitext"));
    assert!(audit.contains("Cell.psc"));
    assert!(audit.contains("errors for a non-optional alias"));
}

#[test]
fn invalid_cell_owner_type_does_not_clear_the_existing_owner() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 1));
    plugin.push_top(Record::new(Tag::new(b"FACT"), 2));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 3).sub(Tag::new(b"XOWN"), 1_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("scr56-breaker.esp", plugin).expect("plugin loads");
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Scr56CellProbe", "Cell")
        .function(
            FunctionBuilder::new("Run", "None")
                .param("owner", "Form")
                .local("result", "None")
                .ins(
                    CallMethod,
                    vec![s("SetActorOwner"), ident("self"), ident("result"), ident("owner")],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).expect("script loads");
    let cell = game.object_of_form(GlobalFormId { file: 0, object: 3 });
    let wrong_owner = game.object_of_form(GlobalFormId { file: 0, object: 2 });
    game.vm.attach(cell, "Scr56CellProbe").expect("attach script");
    let thread = game.vm.call(cell, "Run", &[Value::Object(wrong_owner)]).expect("call starts");
    game.run_until_idle(16);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert_eq!(
        game.world_mut().cell_state(GlobalFormId { file: 0, object: 3 }).unwrap().owner,
        Some(GlobalFormId { file: 0, object: 1 }),
        "an argument outside ActorBase type must leave ownership unchanged"
    );
}
