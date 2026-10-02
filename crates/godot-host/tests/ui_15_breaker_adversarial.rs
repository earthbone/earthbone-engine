// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

const ACTOR: u32 = 0x800;
const ACTOR_REF: u32 = 0x801;
const ITEM: u32 = 0x802;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR));
    plugin.push_top(Record::new(T(b"ACHR"), ACTOR_REF).sub(T(b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(
        Record::new(T(b"MISC"), ITEM)
            .zsub(T(b"FULL"), "Gift")
            .sub(T(b"DATA"), 11_i32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("GiftBreaker.esm", plugin).unwrap();
    Game::new(order)
}

fn start_gift(game: &mut Game) -> soe_papyrus::ThreadId {
    game.load_script(
        &ScriptBuilder::new("Actor", "ObjectReference")
            .function(
                FunctionBuilder::new("ShowGiftMenu", "Int")
                    .param("giving", "Bool")
                    .param("filter", "FormList")
                    .param("stolen", "Bool")
                    .param("favor", "Bool")
                    .native()
                    .build(),
            )
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
        &ScriptBuilder::new("GiftBreaker", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let actor = Value::Object(game.object_of_form(id(ACTOR_REF)));
    game.vm
        .call_global(
            "GiftBreaker",
            "Run",
            &[
                actor,
                Value::Bool(true),
                Value::None,
                Value::Bool(false),
                Value::Bool(true),
            ],
        )
        .unwrap()
}

#[test]
fn ui_15_breaker_latent_result_accumulates_each_transfer_and_stays_nonnegative() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(ITEM), 3);
    let thread = start_gift(&mut game);
    assert!(game.run_until_idle(32));
    assert_eq!(game.gift_menu_transfer(id(ITEM), 1), 1);
    assert_eq!(game.gift_menu_transfer(id(ITEM), 1), 1);
    assert_eq!(game.gift_menu_transfer(id(ITEM), -1), 0);
    assert!(game.close_gift_menu());
    assert_eq!(game.vm.thread_result(thread), Some(Value::Int(22)));
}

#[test]
fn ui_15_breaker_closing_without_a_gift_returns_zero() {
    let mut game = game();
    let thread = start_gift(&mut game);
    assert!(game.run_until_idle(32));
    assert!(game.close_gift_menu());
    assert_eq!(game.vm.thread_result(thread), Some(Value::Int(0)));
}
