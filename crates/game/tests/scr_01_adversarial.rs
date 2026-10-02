// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-01 inventory-filter boundary checks against the ActiveMagicEffect PSC.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallMethod;
use soe_papyrus::{ObjectId, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 1..=2 {
        plugin.push_top(Record::new(Tag(*b"STAT"), object));
    }
    let mut load_order = LoadOrder::new();
    load_order.add("SCR01.esm", plugin).unwrap();
    Game::new(load_order)
}

fn filter_probe() -> Vec<u8> {
    let filter_call = |function: &str, native: &str, takes_filter: bool| {
        let mut builder = FunctionBuilder::new(function, "None").local("result", "Var");
        let mut operands = vec![s(native), ident("self"), ident("result")];
        if takes_filter {
            builder = builder.param("filter", "Form");
            operands.push(ident("filter"));
        }
        builder.ins(CallMethod, operands).build()
    };
    ScriptBuilder::new("Scr01FilterProbe", "ActiveMagicEffect")
        .function(filter_call("Add", "AddInventoryEventFilter", true))
        .function(filter_call("Remove", "RemoveInventoryEventFilter", true))
        .function(filter_call(
            "RemoveAll",
            "RemoveAllInventoryEventFilters",
            false,
        ))
        .bytes()
}

fn invoke(game: &mut Game, listener: ObjectId, method: &str, filter: Option<GlobalFormId>) {
    let args = filter
        .map(|form| vec![Value::Object(game.object_of_form(form))])
        .unwrap_or_default();
    let thread = game.vm.call(listener, method, &args).unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
}

#[test]
fn scr_01_filter_remove_and_save_load_preserve_listener_scoping() {
    let first = id(1);
    let second = id(2);
    let mut game = fixture();
    game.load_script(&filter_probe()).unwrap();
    let listener = game.vm.create_object("listener");
    let other_listener = game.vm.create_object("other_listener");
    game.vm.attach(listener, "Scr01FilterProbe").unwrap();
    game.vm.attach(other_listener, "Scr01FilterProbe").unwrap();
    let first_arg = Some(first);
    let second_arg = Some(second);
    invoke(&mut game, listener, "Add", first_arg);
    invoke(&mut game, listener, "Add", second_arg);
    invoke(&mut game, other_listener, "Add", first_arg);
    assert_eq!(
        game.world().inventory_filters[&listener],
        vec![first, second]
    );
    invoke(&mut game, listener, "Remove", first_arg);
    assert_eq!(game.world().inventory_filters[&listener], vec![second]);
    assert_eq!(game.world().inventory_filters[&other_listener], vec![first]);
    invoke(&mut game, listener, "Add", None);
    assert_eq!(game.world().inventory_filters[&listener], vec![second]);
    invoke(&mut game, listener, "RemoveAll", None);
    assert!(!game.world().inventory_filters.contains_key(&listener));
    let saved = game.world_mut().save_state();
    let loaded = Game::new(soe_esm::LoadOrder::new());
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        loaded.world().inventory_filters[&other_listener],
        vec![first]
    );
}
