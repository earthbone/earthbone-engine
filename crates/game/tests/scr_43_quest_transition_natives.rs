// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-43 checks the Quest startup and shutdown status native contracts.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::quest;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const QUEST: u32 = 0x900;
const BROKEN_QUEST: u32 = 0x901;
const EDID: Tag = Tag::new(b"EDID");

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(QUEST, "TransitionQuest", quest::flags::START_GAME_ENABLED)
            .zsub(T(b"FULL"), "Transition quest"),
    );
    plugin.push_top(
        Record::new(T(b"QUST"), BROKEN_QUEST)
            .zsub(EDID, "BrokenQuest")
            .sub(T(b"DNAM"), [0u8]),
    );
    let mut order = LoadOrder::new();
    order
        .add("Scr43.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn call_quest(game: &mut Game, quest_name: &str, name: &str) -> Value {
    let probe = format!("Scr43Probe{}", PROBE_ID.fetch_add(1, Ordering::SeqCst));
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Quest")
        .local("result", "Var")
        .ins(CallMethod, vec![s(name), ident("target"), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&probe, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let quest = game.form(quest_name).unwrap();
    let object = game.object_of_form(quest);
    let thread = game
        .vm
        .call_global(&probe, "Run", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_43_is_starting_returns_bool_without_changing_running_state() {
    let mut game = game();

    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "IsStarting"),
        Value::Bool(false)
    );
    assert_eq!(
        call_quest(&mut game, "BrokenQuest", "IsStarting"),
        Value::Bool(false)
    );
    assert!(game.traces().is_empty());

    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "Start"),
        Value::Bool(true)
    );
    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "IsStarting"),
        Value::Bool(false)
    );
    let quest = game.form("TransitionQuest").unwrap();
    assert!(game.world().quest(quest).unwrap().running);
    assert!(game.traces().is_empty());
}

#[test]
fn scr_43_is_stopping_returns_bool_without_changing_stopped_state() {
    let mut game = game();

    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "IsStopping"),
        Value::Bool(false)
    );
    assert_eq!(
        call_quest(&mut game, "BrokenQuest", "IsStopping"),
        Value::Bool(false)
    );
    assert!(game.traces().is_empty());

    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "Start"),
        Value::Bool(true)
    );
    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "Stop"),
        Value::None
    );
    assert_eq!(
        call_quest(&mut game, "TransitionQuest", "IsStopping"),
        Value::Bool(false)
    );
    let quest = game.form("TransitionQuest").unwrap();
    assert!(!game.world().quest(quest).unwrap().running);
    assert!(game.traces().is_empty());
}
