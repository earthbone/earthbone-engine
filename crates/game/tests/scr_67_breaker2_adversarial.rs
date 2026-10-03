// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const ITEM: u32 = 0x801;
const OWNER: u32 = 0x802;
const SOURCE: u32 = 0x803;
const TARGET: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), ITEM));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x805));
    plugin.push_top(Record::new(Tag(*b"REFR"), SOURCE).sub(Tag(*b"NAME"), 0x805u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), TARGET).sub(Tag(*b"NAME"), 0x805u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr67Extra.esp", plugin).unwrap();
    Game::new(order)
}

fn remove_all(game: &mut Game, keep_owner: bool) {
    let script = ScriptBuilder::new("Scr67KeepOwnershipProbe", "Form")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("source", "Form")
                .param("target", "Form")
                .param("keep", "Bool")
                .param("removeQuestItems", "Bool")
                .local("result", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("RemoveAllItems"),
                        ident("source"),
                        ident("result"),
                        ident("target"),
                        ident("keep"),
                        ident("removeQuestItems"),
                    ],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let source = Value::Object(game.object_of_form(id(SOURCE)));
    let target = Value::Object(game.object_of_form(id(TARGET)));
    let thread = game
        .vm
        .call_global(
            "Scr67KeepOwnershipProbe",
            "Run",
            &[source, target, Value::Bool(keep_owner), Value::Bool(false)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn source_with_owned_item(game: &mut Game) {
    game.world_mut()
        .reference(id(SOURCE))
        .unwrap()
        .inventory
        .restore(
            id(ITEM),
            1,
            vec![Stack::new(
                1,
                vec![Extra {
                    kind: 0x21,
                    fields: vec![Field::Form(Some(id(OWNER)))],
                }],
            )
            .unwrap()],
        )
        .unwrap();
}

#[test]
fn scr_67_remove_all_items_preserves_stack_ownership_when_requested() {
    let mut game = game();
    let owner = id(OWNER);
    source_with_owned_item(&mut game);

    remove_all(&mut game, true);

    let mut world = game.world_mut();
    let target = world.reference(id(TARGET)).unwrap();
    assert_eq!(target.inventory.count(id(ITEM)), 1);
    assert_eq!(
        target.inventory.stacks(id(ITEM))[0].extra(0x21),
        Some(&Extra {
            kind: 0x21,
            fields: vec![Field::Form(Some(owner))],
        }),
        "CK RemoveAllItems(abKeepOwnership = true) preserves the item's owner"
    );
}

#[test]
fn scr_67_remove_all_items_drops_stack_ownership_when_not_requested() {
    let mut game = game();
    source_with_owned_item(&mut game);

    remove_all(&mut game, false);

    let mut world = game.world_mut();
    let target = world.reference(id(TARGET)).unwrap();
    assert_eq!(target.inventory.count(id(ITEM)), 1);
    assert!(
        target.inventory.stacks(id(ITEM))[0].extra(0x21).is_none(),
        "CK RemoveAllItems defaults abKeepOwnership to false"
    );
}
