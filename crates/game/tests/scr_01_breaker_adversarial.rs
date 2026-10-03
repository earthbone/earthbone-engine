// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Additional argument and listener isolation checks for SCR-01.

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
    for object in 1..=3 {
        plugin.push_top(Record::new(Tag(*b"STAT"), object));
    }
    let mut load_order = LoadOrder::new();
    load_order.add("SCR01Breaker.esm", plugin).unwrap();
    Game::new(load_order)
}

fn probe() -> Vec<u8> {
    let filter_call = |function: &str, native: &str, takes_filter: bool| {
        let mut builder = FunctionBuilder::new(function, "None").local("result", "Var");
        let mut operands = vec![s(native), ident("self"), ident("result")];
        if takes_filter {
            builder = builder.param("filter", "Form");
            operands.push(ident("filter"));
        }
        builder.ins(CallMethod, operands).build()
    };
    ScriptBuilder::new("Scr01BreakerProbe", "ActiveMagicEffect")
        .function(filter_call("Add", "AddInventoryEventFilter", true))
        .function(filter_call("Remove", "RemoveInventoryEventFilter", true))
        .function(filter_call(
            "RemoveAll",
            "RemoveAllInventoryEventFilters",
            false,
        ))
        .bytes()
}

fn invoke(game: &mut Game, listener: ObjectId, method: &str, arg: Value) {
    let thread = game.vm.call(listener, method, &[arg]).unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
}

#[test]
fn non_form_add_argument_logs_native_error_and_preserves_existing_filter() {
    let filter = id(1);
    let mut game = fixture();
    game.load_script(&probe()).unwrap();
    let listener = game.vm.create_object("listener");
    game.vm.attach(listener, "Scr01BreakerProbe").unwrap();

    let filter_value = Value::Object(game.object_of_form(filter));
    invoke(&mut game, listener, "Add", filter_value);
    assert_eq!(game.world().inventory_filters[&listener], vec![filter]);

    invoke(&mut game, listener, "Add", Value::Int(9));

    assert_eq!(game.world().inventory_filters[&listener], vec![filter]);
    assert!(game
        .vm
        .log
        .contains("ActiveMagicEffect.AddInventoryEventFilter: filter must be a non-None Form"));
}

#[test]
fn removing_unknown_filter_and_repeating_remove_all_are_scoped_and_idempotent() {
    let retained = id(1);
    let unknown = id(3);
    let mut game = fixture();
    game.load_script(&probe()).unwrap();
    let listener = game.vm.create_object("listener");
    let other = game.vm.create_object("other_listener");
    game.vm.attach(listener, "Scr01BreakerProbe").unwrap();
    game.vm.attach(other, "Scr01BreakerProbe").unwrap();

    let retained_value = Value::Object(game.object_of_form(retained));
    let unknown_value = Value::Object(game.object_of_form(unknown));
    invoke(&mut game, listener, "Add", retained_value.clone());
    invoke(&mut game, other, "Add", retained_value);
    invoke(&mut game, listener, "Remove", unknown_value);
    assert_eq!(game.world().inventory_filters[&listener], vec![retained]);
    assert_eq!(game.world().inventory_filters[&other], vec![retained]);

    invoke(&mut game, listener, "RemoveAll", Value::None);
    invoke(&mut game, listener, "RemoveAll", Value::None);
    assert!(!game.world().inventory_filters.contains_key(&listener));
    assert_eq!(game.world().inventory_filters[&other], vec![retained]);
}
