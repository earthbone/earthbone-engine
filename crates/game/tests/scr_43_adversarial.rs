// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-43 adversarial checks for Quest transition queries.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::quest::state::LifecycleTask;
use soe_game::{Game, quest};
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use soe_papyrus::{Opcode, Value};

const QUEST: u32 = 0x900;
const OTHER_QUEST: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(quest::build_record(
        QUEST,
        "StartingQuest",
        quest::flags::START_GAME_ENABLED,
    ));
    plugin.push_top(quest::build_record(
        OTHER_QUEST,
        "StoppingQuest",
        quest::flags::START_GAME_ENABLED,
    ));
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0x902)
            .zsub(Tag(*b"EDID"), "MalformedQuest")
            .sub(Tag(*b"DNAM"), [0u8]),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "Scr43Adversarial.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    Game::new(order)
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn query(game: &mut Game, form: GlobalFormId, method: &str) -> Value {
    let probe = format!(
        "Scr43AdversarialProbe{}",
        PROBE_ID.fetch_add(1, Ordering::SeqCst)
    );
    let function = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("target", "Quest")
        .local("result", "Bool")
        .ins(
            Opcode::CallMethod,
            vec![s(method), ident("target"), ident("result")],
        )
        .ins(Opcode::Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&probe, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(form);
    let thread = game
        .vm
        .call_global(&probe, "Run", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(32);
    game.vm
        .thread_result(thread)
        .expect("query should finish without a Papyrus error")
}

#[test]
fn scr_43_queries_only_the_matching_transition_and_are_observational() {
    let mut game = game();
    game.world_mut()
        .quest_state(id(QUEST))
        .unwrap()
        .lifecycle_task = Some(LifecycleTask::Starting);
    game.world_mut()
        .quest_state(id(OTHER_QUEST))
        .unwrap()
        .lifecycle_task = Some(LifecycleTask::Stopping);

    for _ in 0..3 {
        assert_eq!(query(&mut game, id(QUEST), "IsStarting"), Value::Bool(true));
        assert_eq!(
            query(&mut game, id(QUEST), "IsStopping"),
            Value::Bool(false)
        );
        assert_eq!(
            query(&mut game, id(OTHER_QUEST), "IsStarting"),
            Value::Bool(false)
        );
        assert_eq!(
            query(&mut game, id(OTHER_QUEST), "IsStopping"),
            Value::Bool(true)
        );
    }
    assert_eq!(
        game.world().quest(id(QUEST)).unwrap().lifecycle_task,
        Some(LifecycleTask::Starting)
    );
    assert_eq!(
        game.world().quest(id(OTHER_QUEST)).unwrap().lifecycle_task,
        Some(LifecycleTask::Stopping)
    );
}

#[test]
fn scr_43_malformed_and_missing_quest_records_return_false_without_errors() {
    let mut game = game();
    let absent = id(0x999);
    for method in ["IsStarting", "IsStopping"] {
        assert_eq!(query(&mut game, id(0x902), method), Value::Bool(false));
        assert_eq!(query(&mut game, absent, method), Value::Bool(false));
    }
    assert!(game.world().quest(absent).is_none());
}

#[test]
fn scr_43_transition_queries_survive_engine_save_and_load() {
    let original = game();
    original
        .world_mut()
        .quest_state(id(QUEST))
        .unwrap()
        .lifecycle_task = Some(LifecycleTask::Starting);
    original
        .world_mut()
        .quest_state(id(OTHER_QUEST))
        .unwrap()
        .lifecycle_task = Some(LifecycleTask::Stopping);

    let bytes = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        query(&mut restored, id(QUEST), "IsStarting"),
        Value::Bool(true)
    );
    assert_eq!(
        query(&mut restored, id(QUEST), "IsStopping"),
        Value::Bool(false)
    );
    assert_eq!(
        query(&mut restored, id(OTHER_QUEST), "IsStarting"),
        Value::Bool(false)
    );
    assert_eq!(
        query(&mut restored, id(OTHER_QUEST), "IsStopping"),
        Value::Bool(true)
    );
}

#[test]
fn scr_43_query_changes_only_after_the_lifecycle_scheduler_resumes() {
    let mut game = game();
    game.world_mut()
        .quest_state(id(QUEST))
        .unwrap()
        .lifecycle_task = Some(LifecycleTask::Starting);
    assert_eq!(query(&mut game, id(QUEST), "IsStarting"), Value::Bool(true));

    game.tick(0.25);

    assert_eq!(
        query(&mut game, id(QUEST), "IsStarting"),
        Value::Bool(false)
    );
    assert!(game.world().quest(id(QUEST)).unwrap().running);
}
