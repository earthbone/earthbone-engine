// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-15 gift menu requests stay latent and transfer in either direction.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{ThreadState, Value};

const ACTOR: u32 = 0x800;
const ITEM: u32 = 0x801;
const FILTER: u32 = 0x802;
const ACTOR_REF: u32 = 0x803;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR).zsub(T(b"FULL"), "Recipient"));
    plugin.push_top(Record::new(T(b"ACHR"), ACTOR_REF).sub(T(b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).sub(T(b"DATA"), 25_i32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"FLST"), FILTER).sub(T(b"LNAM"), ITEM.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Gift.esm", plugin).unwrap();
    Game::new(order)
}

fn open(game: &mut Game, giving: bool, use_favor: bool) -> soe_papyrus::ThreadId {
    let actor_native = FunctionBuilder::new("ShowGiftMenu", "Int")
        .param("giving", "Bool")
        .param("filter", "FormList")
        .param("showStolen", "Bool")
        .param("favor", "Bool")
        .native()
        .build();
    game.load_script(
        &ScriptBuilder::new("Actor", "ObjectReference")
            .function(actor_native)
            .bytes(),
    )
    .unwrap();
    let function = FunctionBuilder::new("Run", "Int")
        .global()
        .param("actor", "Actor")
        .param("filter", "FormList")
        .param("giving", "Bool")
        .param("showStolen", "Bool")
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
                ident("showStolen"),
                ident("favor"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("UI15Probe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let actor = Value::Object(game.object_of_form(id(ACTOR_REF)));
    let filter = Value::Object(game.object_of_form(id(FILTER)));
    game.vm
        .call_global(
            "UI15Probe",
            "Run",
            &[
                actor,
                filter,
                Value::Bool(giving),
                Value::Bool(false),
                Value::Bool(use_favor),
            ],
        )
        .unwrap()
}

#[test]
fn ui_15_gift_menu_filters_giving_and_taking_and_resumes_with_nonnegative_value() {
    for (giving, use_favor) in [(true, true), (false, true), (true, false)] {
        let mut game = game();
        let player = game.player();
        {
            let mut world = game.world_mut();
            world.add_item(if giving { player } else { id(ACTOR_REF) }, id(ITEM), 2);
        }
        let thread = open(&mut game, giving, use_favor);
        assert!(game.run_until_idle(64));
        let request = game.gift_menu_request().unwrap();
        assert_eq!(request.filter, Some(id(FILTER)));
        assert_eq!(request.source, if giving { player } else { id(ACTOR_REF) });
        assert_eq!(request.target, if giving { id(ACTOR_REF) } else { player });
        assert!(matches!(
            game.vm.thread_state(thread),
            Some(ThreadState::Latent(_))
        ));
        assert_eq!(game.gift_menu_transfer(id(ITEM + 1), 1), 0);
        assert_eq!(game.gift_menu_transfer(id(ITEM), 1), 1);
        assert_eq!(
            game.gift_menu_request().unwrap().favor_points,
            if use_favor { 25 } else { 0 }
        );
        assert_eq!(game.world_mut().item_count(request.source, id(ITEM)), 1);
        assert_eq!(game.world_mut().item_count(request.target, id(ITEM)), 1);
        assert!(game.close_gift_menu());
        assert_eq!(game.vm.thread_state(thread), Some(ThreadState::Done));
        assert_eq!(
            game.vm.thread_result(thread),
            Some(Value::Int(if use_favor { 25 } else { 0 }))
        );
    }
}
