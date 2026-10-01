// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x901;
const ACTOR_REF: u32 = 0x902;
const ITEM: u32 = 0x903;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR));
    plugin.push_top(Record::new(T(b"ACHR"), ACTOR_REF).sub(T(b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).sub(T(b"DATA"), 7_i32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("GiftAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn call_gift(game: &mut Game, giver: GlobalFormId, giving: bool) -> soe_papyrus::ThreadId {
    let native = FunctionBuilder::new("ShowGiftMenu", "Int")
        .param("giving", "Bool")
        .param("filter", "FormList")
        .param("stolen", "Bool")
        .param("favor", "Bool")
        .native()
        .build();
    game.load_script(
        &ScriptBuilder::new("Actor", "ObjectReference")
            .function(native)
            .bytes(),
    )
    .unwrap();
    let function = FunctionBuilder::new("Run", "Int")
        .global()
        .param("actor", "Actor")
        .param("giving", "Bool")
        .param("filter", "FormList")
        .param("stolen", "Bool")
        .param("favor", "Bool")
        .param("result", "Int")
        .ins(
            CallMethod,
            vec![
                s("ShowGiftMenu"),
                ident("actor"),
                ident("result"),
                ident("giving"),
                ident("filter"),
                ident("stolen"),
                ident("favor"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("GiftAdversarial", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let actor = Value::Object(game.object_of_form(giver));
    game.vm
        .call_global(
            "GiftAdversarial",
            "Run",
            &[
                actor,
                Value::Bool(giving),
                Value::None,
                Value::Bool(false),
                Value::Bool(true),
            ],
        )
        .unwrap()
}

#[test]
fn ui_15_adversarial_rejects_negative_and_excess_transfers_without_changing_value() {
    for giving in [true, false] {
        let mut game = game();
        let player = game.player();
        let source = if giving { player } else { id(ACTOR_REF) };
        game.world_mut().add_item(source, id(ITEM), 1);
        call_gift(&mut game, id(ACTOR_REF), giving);
        assert!(game.run_until_idle(32));
        assert_eq!(game.gift_menu_transfer(id(ITEM), -1), 0);
        assert_eq!(game.gift_menu_transfer(id(ITEM), 3), 1);
        assert_eq!(game.gift_menu_request().unwrap().favor_points, 7);
        assert_eq!(game.world_mut().item_count(source, id(ITEM)), 0);
    }
}

#[test]
fn ui_15_adversarial_empty_or_unavailable_item_keeps_the_request_pending() {
    let mut game = game();
    call_gift(&mut game, id(ACTOR_REF), true);
    assert!(game.run_until_idle(32));
    let first = game.gift_menu_request().unwrap().clone();
    assert_eq!(game.gift_menu_transfer(id(ITEM), 1), 0);
    assert_eq!(game.gift_menu_request().unwrap(), first);
}
