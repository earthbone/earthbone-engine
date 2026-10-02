// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const FILTER: u32 = 0x801;
const KEYWORD: u32 = 0x802;

fn fixture(gift_filter: Option<Vec<u8>>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut base = Record::new(T(b"NPC_"), BASE);
    if let Some(bytes) = gift_filter {
        base = base.sub(T(b"GNAM"), bytes);
    }
    plugin.push_top(base);
    plugin.push_top(Record::new(T(b"FLST"), FILTER).sub(T(b"LNAM"), KEYWORD.to_le_bytes()));
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD));
    let mut order = LoadOrder::new();
    order.add("Scr18.esm", plugin).unwrap();
    Game::new(order)
}

fn invoke_filter(game: &mut Game, base: GlobalFormId) -> Value {
    let script = "Scr18GiftFilterProbe";
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("base", "ActorBase")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![s("GetGiftFilter"), ident("base"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let actor_base = Value::Object(game.object_of_form(base));
    let thread = game.vm.call_global(script, "Run", &[actor_base]).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_18_get_gift_filter_returns_the_authored_form_list() {
    let mut game = fixture(Some(FILTER.to_le_bytes().to_vec()));
    let result = invoke_filter(&mut game, id(BASE));
    assert_eq!(
        result.as_object(),
        Some(game.object_of_form(id(FILTER))),
        "GetGiftFilter takes no arguments and returns NPC_.GNAM as a FormList"
    );
    assert_eq!(
        game.world_mut().form_list(id(FILTER)).unwrap().as_slice(),
        &[id(KEYWORD)],
        "the returned FormList exposes the authored keyword"
    );
    assert!(
        game.traces()
            .iter()
            .all(|trace| !trace.contains("GetGiftFilter")),
        "a valid gift filter must not log a native error"
    );
}

#[test]
fn scr_18_get_gift_filter_returns_none_when_the_optional_link_is_absent() {
    let mut game = fixture(None);
    assert_eq!(invoke_filter(&mut game, id(BASE)), Value::None);
    assert!(
        game.traces()
            .iter()
            .all(|trace| !trace.contains("GetGiftFilter")),
        "an absent optional GNAM link is not an error"
    );
}

#[test]
fn scr_18_get_gift_filter_rejects_malformed_and_non_form_list_links() {
    let mut malformed = fixture(Some(vec![1, 2, 3]));
    assert_eq!(invoke_filter(&mut malformed, id(BASE)), Value::None);
    assert!(
        malformed
            .traces()
            .iter()
            .any(|trace| trace.contains("ActorBase.GetGiftFilter")),
        "malformed GNAM data must report a native error"
    );
    assert!(
        malformed
            .world()
            .actor_base_gift_filter(id(BASE))
            .unwrap_err()
            .to_string()
            .contains("GNAM"),
        "the owning World API must return a typed parse error"
    );

    let mut wrong_type = fixture(Some(KEYWORD.to_le_bytes().to_vec()));
    assert_eq!(invoke_filter(&mut wrong_type, id(BASE)), Value::None);
    assert!(
        wrong_type
            .traces()
            .iter()
            .any(|trace| trace.contains("not a live FLST")),
        "GNAM must resolve to a live FLST"
    );
}

#[test]
fn scr_18_get_gift_filter_reads_a_vanilla_actor_base_when_game_data_is_set() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed ActorBase gift filter");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).unwrap();
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let candidates: Vec<_> = order.winners_of(T(b"NPC_")).map(|(id, _)| id).collect();
    let game = Game::new(order);
    let base = candidates.into_iter().find_map(|id| {
        game.world()
            .actor_base_gift_filter(id)
            .ok()
            .flatten()
            .map(|_| id)
    });
    let base = base.expect("Skyrim.esm has an NPC_ record with a valid GNAM gift filter");
    let expected = game
        .world()
        .actor_base_gift_filter(base)
        .unwrap()
        .expect("selected vanilla ActorBase has a gift filter");
    let mut game = game;
    let result = invoke_filter(&mut game, base);
    assert_eq!(
        result.as_object(),
        Some(game.object_of_form(expected)),
        "the native returns the resolved vanilla FormList"
    );
}
