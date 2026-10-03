// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

#[test]
fn remove_all_inventory_filters_rejects_extra_arguments() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 1));
    let mut order = LoadOrder::new();
    order.add("Scr01Arg.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let bytes = ScriptBuilder::new("Scr01ArgProbe", "ActiveMagicEffect")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("extra", "Bool")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("RemoveAllInventoryEventFilters"),
                        ident("self"),
                        ident("result"),
                        ident("extra"),
                    ],
                )
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let listener = game.vm.create_object("listener");
    game.vm.attach(listener, "Scr01ArgProbe").unwrap();
    game.world_mut()
        .add_inventory_event_filter(listener, soe_esm::GlobalFormId { file: 0, object: 1 });
    let thread = game.vm.call(listener, "Run", &[Value::Bool(true)]).unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(
        game.world().inventory_filters.get(&listener),
        Some(&vec![soe_esm::GlobalFormId { file: 0, object: 1 }]),
        "an invalid call must not clear the listener's filter"
    );
    assert!(
        game.vm
            .log
            .contains("RemoveAllInventoryEventFilters expects no arguments"),
        "CK declares RemoveAllInventoryEventFilters() with no parameters"
    );
}

#[test]
fn add_and_remove_inventory_filters_reject_extra_arguments() {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 1..=2 {
        plugin.push_top(Record::new(Tag(*b"STAT"), object));
    }
    let mut order = LoadOrder::new();
    order.add("Scr01FilterArgs.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let function = |name: &str, native: &str| {
        FunctionBuilder::new(name, "Var")
            .global()
            .param("filter", "Form")
            .param("extra", "Bool")
            .local("result", "Var")
            .ins(
                CallMethod,
                vec![
                    s(native),
                    ident("self"),
                    ident("result"),
                    ident("filter"),
                    ident("extra"),
                ],
            )
            .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
            .build()
    };
    let bytes = ScriptBuilder::new("Scr01FilterArgsProbe", "ActiveMagicEffect")
        .function(function("Add", "AddInventoryEventFilter"))
        .function(function("Remove", "RemoveInventoryEventFilter"))
        .bytes();
    game.load_script(&bytes).unwrap();
    let listener = game.vm.create_object("listener");
    game.vm.attach(listener, "Scr01FilterArgsProbe").unwrap();
    let first = soe_esm::GlobalFormId { file: 0, object: 1 };
    let second = soe_esm::GlobalFormId { file: 0, object: 2 };
    game.world_mut().add_inventory_event_filter(listener, first);
    let first_arg = Value::Object(game.object_of_form(first));
    let second_arg = Value::Object(game.object_of_form(second));
    let thread = game
        .vm
        .call(listener, "Add", &[second_arg, Value::Bool(true)])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(
        game.world().inventory_filters.get(&listener),
        Some(&vec![first])
    );
    assert!(game
        .vm
        .log
        .contains("AddInventoryEventFilter expects one Form argument"));
    let thread = game
        .vm
        .call(listener, "Remove", &[first_arg, Value::Bool(true)])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(
        game.world().inventory_filters.get(&listener),
        Some(&vec![first])
    );
    assert!(game
        .vm
        .log
        .contains("RemoveInventoryEventFilter expects one Form argument"));
}

#[test]
fn remove_inventory_filter_rejects_extra_arguments_without_removing_filter() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 1));
    let mut order = LoadOrder::new();
    order.add("Scr01RemoveArgs.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let bytes = ScriptBuilder::new("Scr01RemoveArgsProbe", "ActiveMagicEffect")
        .function(
            FunctionBuilder::new("Run", "Var")
                .global()
                .param("filter", "Form")
                .param("extra", "Bool")
                .local("result", "Var")
                .ins(
                    CallMethod,
                    vec![
                        s("RemoveInventoryEventFilter"),
                        ident("self"),
                        ident("result"),
                        ident("filter"),
                        ident("extra"),
                    ],
                )
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let listener = game.vm.create_object("listener");
    game.vm.attach(listener, "Scr01RemoveArgsProbe").unwrap();
    let filter = soe_esm::GlobalFormId { file: 0, object: 1 };
    game.world_mut()
        .add_inventory_event_filter(listener, filter);
    let filter_value = Value::Object(game.object_of_form(filter));
    let thread = game
        .vm
        .call(listener, "Run", &[filter_value, Value::Bool(true)])
        .unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
    assert_eq!(
        game.world().inventory_filters.get(&listener),
        Some(&vec![filter])
    );
    assert!(game
        .vm
        .log
        .contains("RemoveInventoryEventFilter expects one Form argument"));
}
