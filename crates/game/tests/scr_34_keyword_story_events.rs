// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    quest::state::{StoryEvent, StoryParameter},
};
use soe_papyrus::{
    Opcode::*,
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag(bytes)
}

const KEYWORD: u32 = 0x800;
const LOCATION: u32 = 0x801;
const REFERENCE_ONE: u32 = 0x802;
const REFERENCE_TWO: u32 = 0x803;
const QUEST: u32 = 0x804;
const EVENT_NODE: u32 = 0x805;
const QUEST_NODE: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(with_quest: bool, malformed: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"KYWD"), KEYWORD).zsub(tag(*b"EDID"), "ProbeKeyword"));
    plugin.push_top(Record::new(tag(*b"LCTN"), LOCATION).zsub(tag(*b"EDID"), "ProbeLocation"));
    plugin.push_top(Record::new(tag(*b"REFR"), REFERENCE_ONE));
    plugin.push_top(Record::new(tag(*b"REFR"), REFERENCE_TWO));
    if with_quest {
        plugin.push_top(Record::new(tag(*b"QUST"), QUEST).sub(tag(*b"DNAM"), [0; 12]));
        let mut root = Record::new(tag(*b"SMEN"), EVENT_NODE)
            .sub(tag(*b"ENAM"), u32::from_le_bytes(*b"SCPT").to_le_bytes());
        if malformed {
            root = root.sub(tag(*b"CTDA"), []);
        }
        plugin.push_top(root);
        plugin.push_top(
            Record::new(tag(*b"SMQN"), QUEST_NODE)
                .sub(tag(*b"PNAM"), EVENT_NODE.to_le_bytes())
                .sub(tag(*b"NNAM"), QUEST.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Scr34.esm", plugin).unwrap();
    Game::new(order)
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn call(game: &mut Game, target: Value, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr34Probe{}", PROBE_ID.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Keyword")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let argument = format!("arg{index}");
        function = function.param(
            &argument,
            if matches!(value, Value::Int(_)) {
                "Int"
            } else {
                "Form"
            },
        );
        operands.push(ident(&argument));
    }
    let function = function
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let mut all = vec![target];
    all.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &all).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn form(game: &mut Game, form: GlobalFormId) -> Value {
    Value::Object(game.object_of_form(form))
}

fn args(game: &mut Game) -> Vec<Value> {
    vec![
        form(game, id(LOCATION)),
        form(game, id(REFERENCE_ONE)),
        form(game, id(REFERENCE_TWO)),
        Value::Int(-12),
        Value::Int(34),
    ]
}

fn expected_event() -> StoryEvent {
    StoryEvent {
        id: KEYWORD,
        code: *b"SCPT",
        parameters: vec![
            StoryParameter::Reference {
                kind: 1,
                reference: Some(id(KEYWORD)),
            },
            StoryParameter::Reference {
                kind: 2,
                reference: Some(id(LOCATION)),
            },
            StoryParameter::Reference {
                kind: 0,
                reference: Some(id(REFERENCE_ONE)),
            },
            StoryParameter::Reference {
                kind: 0,
                reference: Some(id(REFERENCE_TWO)),
            },
            StoryParameter::Integer((-12_i32) as u32),
            StoryParameter::Integer(34),
        ],
    }
}

#[test]
fn scr_34_send_story_event_routes_arguments_and_returns_none() {
    let mut game = game(true, false);
    let arguments = args(&mut game);
    let keyword = form(&mut game, id(KEYWORD));
    assert_eq!(
        call(&mut game, keyword, "SendStoryEvent", &arguments),
        Value::None
    );
    let world = game.world();
    let quest = world.quest(id(QUEST)).unwrap();
    assert!(quest.running);
    assert_eq!(
        quest.runtime.run_data.as_ref().unwrap().event.as_ref(),
        Some(&expected_event())
    );
}

#[test]
fn scr_34_send_story_event_and_wait_reports_started_quest_and_arguments() {
    let mut game = game(true, false);
    let arguments = args(&mut game);
    let keyword = form(&mut game, id(KEYWORD));
    assert_eq!(
        call(&mut game, keyword, "SendStoryEventAndWait", &arguments),
        Value::Bool(true),
    );
    let world = game.world();
    let quest = world.quest(id(QUEST)).unwrap();
    assert!(quest.running);
    assert_eq!(
        quest.runtime.run_data.as_ref().unwrap().event.as_ref(),
        Some(&expected_event())
    );
}

#[test]
fn scr_34_send_story_event_and_wait_returns_false_without_a_started_quest() {
    let mut game = game(false, false);
    let arguments = args(&mut game);
    let keyword = form(&mut game, id(KEYWORD));
    assert_eq!(
        call(&mut game, keyword, "SendStoryEventAndWait", &arguments),
        Value::Bool(false),
    );
}

#[test]
fn scr_34_story_event_routing_error_is_logged_and_wait_returns_false() {
    let mut game = game(true, true);
    let arguments = args(&mut game);
    let keyword = form(&mut game, id(KEYWORD));
    assert_eq!(
        call(&mut game, keyword, "SendStoryEventAndWait", &arguments),
        Value::Bool(false),
    );
    assert!(game.vm.log.messages().iter().any(|message| {
        message.contains("Keyword.SendStoryEventAndWait")
            && (message.contains("malformed story node") || message.contains("condition on node"))
    }));
    assert!(
        game.world()
            .quest(id(QUEST))
            .is_none_or(|quest| !quest.running)
    );
}

#[test]
fn scr_34_send_story_event_logs_routing_error_without_starting_a_quest() {
    let mut game = game(true, true);
    let arguments = args(&mut game);
    let keyword = form(&mut game, id(KEYWORD));
    assert_eq!(
        call(&mut game, keyword, "SendStoryEvent", &arguments),
        Value::None,
    );
    assert!(game.vm.log.messages().iter().any(|message| {
        message.contains("Keyword.SendStoryEvent")
            && (message.contains("malformed story node") || message.contains("condition on node"))
    }));
    assert!(
        game.world()
            .quest(id(QUEST))
            .is_none_or(|quest| !quest.running)
    );
}
