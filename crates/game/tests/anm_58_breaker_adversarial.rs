// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! ANM-58 adversarial checks for the host event delivery path.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle};
use soe_papyrus::{
    Opcode::{CallMethod, Return},
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ACTI"), 0x100));
    let mut order = LoadOrder::new();
    order.add("Anm58.esm", plugin).unwrap();
    Game::new(order)
}

fn waiting_thread(game: &mut Game, source: GlobalFormId) -> soe_papyrus::ThreadId {
    let object = game.object_of(Handle::Form(source));
    let function = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("target", "ObjectReference")
        .param("event", "String")
        .local("result", "Bool")
        .ins(
            CallMethod,
            vec![
                ident("WaitForAnimationEvent"),
                ident("target"),
                ident("result"),
                ident("event"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new("Anm58HostWait", "Form")
        .function(function)
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global(
            "Anm58HostWait",
            "Run",
            &[Value::Object(object), Value::string("HitFrame")],
        )
        .unwrap();
    game.run_until_idle(64);
    thread
}

#[test]
fn host_graph_event_reaches_game_once_across_signal_and_notice_paths() {
    let mut game = game();
    let source = id(0x100);

    // SkinnedActorNode queues each event and emits its signal. PlayerNode
    // reports that signal immediately, then drains the queued notice later.
    game.world_mut().queue_animation_report(source, "HitFrame");
    game.report_animation_event(source, "HitFrame");
    game.deliver_animation_reports();

    assert_eq!(game.world().animation_events, [(source, "HitFrame".into())]);
}

#[test]
fn host_graph_signal_resumes_papyrus_wait_true_and_unload_resumes_false() {
    let mut game = game();
    let player = game.player();
    let source = game.world_mut().place_at(id(0x100), player, false);

    let event_wait = waiting_thread(&mut game, source);
    game.world_mut().queue_animation_report(source, "HitFrame");
    game.report_animation_event(source, "HitFrame");
    game.deliver_animation_reports();
    assert_eq!(game.vm.thread_result(event_wait), Some(Value::Bool(true)));

    let unload_wait = waiting_thread(&mut game, source);
    game.world_mut().queue_animation_unload(source);
    game.deliver_animation_reports();
    assert_eq!(game.vm.thread_result(unload_wait), Some(Value::Bool(false)));
}
